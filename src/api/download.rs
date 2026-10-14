//! Service d'un fichier en téléchargement : en-têtes de cache et requêtes
//! partielles (`Range`), pour reprendre un téléchargement interrompu ou
//! lire une vidéo sans la récupérer en entier.

use std::io::SeekFrom;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::http::header::{
    ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, IF_MODIFIED_SINCE, IF_RANGE,
    LAST_MODIFIED, RANGE,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::files::OpenedFile;

#[derive(Debug, PartialEq)]
enum RangeRequest {
    Full,
    /// Bornes incluses.
    Partial(u64, u64),
    Unsatisfiable,
}

/// Interprète un en-tête `Range`. Une demande de plusieurs plages reçoit le
/// fichier entier, ce que la RFC 9110 autorise.
fn parse_range(header: &str, size: u64) -> RangeRequest {
    let Some(spec) = header.trim().strip_prefix("bytes=") else {
        return RangeRequest::Unsatisfiable;
    };
    if spec.contains(',') {
        return RangeRequest::Full;
    }
    let Some((start, end)) = spec.trim().split_once('-') else {
        return RangeRequest::Unsatisfiable;
    };
    let (start, end) = (start.trim(), end.trim());

    if start.is_empty() {
        return match end.parse::<u64>() {
            Ok(n) if n > 0 && size > 0 => RangeRequest::Partial(size - n.min(size), size - 1),
            _ => RangeRequest::Unsatisfiable,
        };
    }
    let Ok(start) = start.parse::<u64>() else {
        return RangeRequest::Unsatisfiable;
    };
    if start >= size {
        return RangeRequest::Unsatisfiable;
    }
    let end = if end.is_empty() {
        size - 1
    } else {
        match end.parse::<u64>() {
            Ok(e) if e >= start => e.min(size - 1),
            _ => return RangeRequest::Unsatisfiable,
        }
    };
    RangeRequest::Partial(start, end)
}

fn truncate_to_secs(t: SystemTime) -> SystemTime {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    UNIX_EPOCH + Duration::from_secs(secs)
}

/// Réponse servant `file` ; `filename` détermine le type MIME.
pub(super) async fn serve_file(headers: &HeaderMap, filename: &str, file: OpenedFile) -> Response {
    let size = file.size;
    let last_modified = file.modified.map(httpdate::fmt_http_date);

    if let (Some(modified), Some(since)) = (
        file.modified,
        headers
            .get(IF_MODIFIED_SINCE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| httpdate::parse_http_date(v).ok()),
    ) {
        if truncate_to_secs(modified) <= since {
            return StatusCode::NOT_MODIFIED.into_response();
        }
    }

    let if_range_matches = match headers.get(IF_RANGE).and_then(|v| v.to_str().ok()) {
        Some(v) => last_modified.as_deref() == Some(v),
        None => true,
    };
    let range = match headers.get(RANGE).and_then(|v| v.to_str().ok()) {
        Some(r) if if_range_matches => parse_range(r, size),
        _ => RangeRequest::Full,
    };

    let mut response = Response::builder().header(ACCEPT_RANGES, "bytes").header(
        CONTENT_TYPE,
        mime_guess::from_path(filename)
            .first_or_octet_stream()
            .as_ref(),
    );
    if let Some(lm) = &last_modified {
        response = response.header(LAST_MODIFIED, lm);
    }

    let (status, start, len) = match range {
        RangeRequest::Full => (StatusCode::OK, 0, size),
        RangeRequest::Partial(start, end) => {
            response = response.header(CONTENT_RANGE, format!("bytes {start}-{end}/{size}"));
            (StatusCode::PARTIAL_CONTENT, start, end - start + 1)
        }
        RangeRequest::Unsatisfiable => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(CONTENT_RANGE, format!("bytes */{size}"))],
            )
                .into_response();
        }
    };

    let mut reader = tokio::fs::File::from_std(file.file);
    if start > 0 {
        if let Err(e) = reader.seek(SeekFrom::Start(start)).await {
            tracing::error!(err = %e, "positionnement dans le fichier à télécharger");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    let body = Body::from_stream(ReaderStream::new(reader.take(len)));
    response
        .status(status)
        .header(CONTENT_LENGTH, HeaderValue::from(len))
        .body(body)
        .expect("en-têtes de réponse valides")
}

/// Encode un nom de fichier comme `url.PathEscape` en Go, pour l'en-tête
/// `Content-Disposition` (les clients existants décodent ce format).
pub(super) fn path_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => out.push(b as char),
            b'-' | b'_' | b'.' | b'~' | b'$' | b'&' | b'+' | b':' | b'=' | b'@' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_range_formes_simples() {
        assert_eq!(
            parse_range("bytes=0-99", 1000),
            RangeRequest::Partial(0, 99)
        );
        assert_eq!(
            parse_range("bytes=900-", 1000),
            RangeRequest::Partial(900, 999)
        );
        assert_eq!(
            parse_range("bytes=-100", 1000),
            RangeRequest::Partial(900, 999)
        );
        assert_eq!(
            parse_range("bytes=-5000", 1000),
            RangeRequest::Partial(0, 999)
        );
        assert_eq!(
            parse_range("bytes=0-5000", 1000),
            RangeRequest::Partial(0, 999)
        );
    }

    #[test]
    fn test_parse_range_hors_limites_ou_invalide_insatisfiable() {
        for header in [
            "bytes=1000-",
            "bytes=50-10",
            "bytes=-0",
            "bytes=abc-",
            "octets=0-1",
            "bytes=5",
        ] {
            assert_eq!(
                parse_range(header, 1000),
                RangeRequest::Unsatisfiable,
                "{header}"
            );
        }
        assert_eq!(parse_range("bytes=0-", 0), RangeRequest::Unsatisfiable);
    }

    #[test]
    fn test_parse_range_plusieurs_plages_fichier_entier() {
        assert_eq!(parse_range("bytes=0-1,5-6", 1000), RangeRequest::Full);
    }

    #[test]
    fn test_path_escape_compatible_go() {
        assert_eq!(path_escape("rapport final.pdf"), "rapport%20final.pdf");
        assert_eq!(path_escape("été\"?.txt"), "%C3%A9t%C3%A9%22%3F.txt");
        assert_eq!(path_escape("a+b=c@d$e&f:g"), "a+b=c@d$e&f:g");
        assert_eq!(path_escape("x/y;z,w"), "x%2Fy%3Bz%2Cw");
    }
}

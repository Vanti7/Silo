//! silo : interface web d'administration pour NAS maison (API REST JSON
//! sous `/api/v1` et frontend embarqué dans le binaire).

pub mod api;
pub mod auth;
pub mod btrfs;
pub mod config;
pub mod docker;
pub mod files;
pub mod serve;
pub mod store;
pub mod system;
pub mod web;

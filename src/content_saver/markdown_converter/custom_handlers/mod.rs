//! Custom tag handlers for html2md conversion
//!
//! This module contains specialized TagHandler implementations that extend
//! html2md's default behavior to preserve additional semantic information.

pub mod code_language_handler;

pub use code_language_handler::create_custom_handlers;

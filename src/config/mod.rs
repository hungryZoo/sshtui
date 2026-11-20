//! OpenSSH configuration parsing, editing, and storage.

pub mod diff;
pub mod edit;
pub mod lexer;
pub mod model;
pub mod reader;
pub mod writer;

pub use edit::{
    BlockFields, Change, EditError, FieldState, HostEdit, NewHost, OptionChanges, OptionKey,
    add_host, apply_host_edit, block_fields, delete_pattern, format_arg,
};
pub use lexer::lex_line;
pub use model::{Arg, Block, BlockKind, Directive, Eol, Line, LineKind, ParsedConfig};
pub use reader::{
    ConfigSet, HostEntry, LoadedFile, ReadError, ReadWarning, is_listable_pattern, load,
    resolve_include_pattern,
};

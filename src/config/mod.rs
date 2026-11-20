//! OpenSSH configuration parsing, editing, and storage.

pub mod diff;
pub mod edit;
pub mod lexer;
pub mod model;
pub mod plan;
pub mod reader;
pub mod writer;

pub use diff::{DiffHunk, DiffLine, DiffTag, diff_lines};
pub use edit::{
    BlockFields, Change, EditError, FieldState, HostEdit, NewHost, OptionChanges, OptionKey,
    add_host, apply_host_edit, block_fields, delete_pattern, format_arg,
};
pub use lexer::lex_line;
pub use model::{Arg, Block, BlockKind, Directive, Eol, Line, LineKind, ParsedConfig};
pub use plan::{DIFF_CONTEXT, PendingWrite, PlanError, plan_add, plan_delete, plan_edit};
pub use reader::{
    ConfigSet, HostEntry, LoadedFile, ReadError, ReadWarning, is_listable_pattern, load,
    resolve_include_pattern,
};
pub use writer::{
    WriteError, WriteOutcome, backup_file_name, create_file, is_writable, replace_file,
};

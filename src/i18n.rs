//! Language selection and localized text for the terminal interface.

use std::ffi::OsString;
use std::path::Path;

use crate::app::{ConfirmKind, FormField};
use crate::config::{OptionKey, ReadWarning};
use crate::ssh::effective::Mismatch;
use crate::update::InstallMethod;
use crate::validate::{Field, ValidationError};

/// Display language of the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// English.
    #[default]
    En,
    /// Korean.
    Ko,
}

/// Environment variable that forces the display language (`ko` or `en`).
pub const LANG_ENV: &str = "SSHTUI_LANG";

/// Classifies a locale name. Returns None for empty, C, and POSIX locales.
pub fn lang_of_locale(locale: &str) -> Option<Lang> {
    let locale = locale.trim();
    let language = locale.split(['_', '-', '.', '@']).next()?;
    if locale.is_empty()
        || language.eq_ignore_ascii_case("C")
        || language.eq_ignore_ascii_case("POSIX")
    {
        None
    } else if language.eq_ignore_ascii_case("ko") {
        Some(Lang::Ko)
    } else {
        Some(Lang::En)
    }
}

/// Chooses the language using the given environment and system locale.
pub fn detect(env: &dyn Fn(&str) -> Option<OsString>, system_locale: Option<&str>) -> Lang {
    if let Some(value) = env(LANG_ENV) {
        if value.eq_ignore_ascii_case("ko") {
            return Lang::Ko;
        }
        if value.eq_ignore_ascii_case("en") {
            return Lang::En;
        }
    }
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(env)
        .find_map(|value| value.to_str().and_then(lang_of_locale))
        .or_else(|| system_locale.and_then(lang_of_locale))
        .unwrap_or_default()
}

/// Returns the operating system's locale.
pub fn system_locale() -> Option<String> {
    sys_locale::get_locale()
}

/// Fixed text shared by the screens, hints, and status messages.
pub(crate) struct Text {
    pub connect: &'static str,
    pub new: &'static str,
    pub edit: &'static str,
    pub delete: &'static str,
    pub forget_known_host: &'static str,
    pub forget: &'static str,
    pub filter: &'static str,
    pub reload: &'static str,
    pub quit: &'static str,
    pub clear_filter: &'static str,
    pub apply_filter: &'static str,
    pub select: &'static str,
    pub filter_hosts: &'static str,
    pub new_connection: &'static str,
    pub new_row: &'static str,
    pub new_description: &'static str,
    pub no_matching_hosts: &'static str,
    pub read_only: &'static str,
    pub read_only_file: &'static str,
    pub enter_runs: &'static str,
    pub update_available: &'static str,
    pub nickname: &'static str,
    pub user: &'static str,
    pub address: &'static str,
    pub port: &'static str,
    pub key_file: &'static str,
    pub key_login: &'static str,
    pub password: &'static str,
    pub source: &'static str,
    pub next: &'static str,
    pub back: &'static str,
    pub clear_field: &'static str,
    pub pick_key: &'static str,
    pub browse: &'static str,
    pub toggle: &'static str,
    pub save_and_connect: &'static str,
    pub review_changes: &'static str,
    pub install_public_key: &'static str,
    pub password_once: &'static str,
    pub disable_password_prompt: &'static str,
    pub check_key_login: &'static str,
    pub optional_save: &'static str,
    pub required: &'static str,
    pub optional: &'static str,
    pub on_enter: &'static str,
    pub save_step: &'static str,
    pub key_step: &'static str,
    pub run_step: &'static str,
    pub connect_without_saving: &'static str,
    pub public_key_placeholder: &'static str,
    pub cancel: &'static str,
    pub choose: &'static str,
    pub back_to_list: &'static str,
    pub scroll: &'static str,
    pub save: &'static str,
    pub back_to_form: &'static str,
    pub cancel_choice: &'static str,
    pub delete_choice: &'static str,
    pub forget_choice: &'static str,
    pub key_picker: &'static str,
    pub use_key: &'static str,
    pub import_history: &'static str,
    pub import_guidance: &'static str,
    pub fill_form: &'static str,
    pub close: &'static str,
    pub connect_anyway: &'static str,
    pub ok: &'static str,
    pub mismatch_title: &'static str,
    pub installed_via: &'static str,
    pub command: &'static str,
    pub download: &'static str,
    pub update_title: &'static str,
    pub update_and_exit: &'static str,
    pub reloaded: &'static str,
    pub no_history_suggestions: &'static str,
    pub needs_key_file: &'static str,
    pub no_changes: &'static str,
    pub no_backup: &'static str,
    pub server_password_prompt: &'static str,
    pub key_install_finished: &'static str,
    pub key_install_failed: &'static str,
    pub return_after_install: &'static str,
}

const EN: Text = Text {
    connect: "connect",
    new: "new",
    edit: "edit",
    delete: "delete",
    forget_known_host: "forget(known_host)",
    forget: "forget",
    filter: "filter",
    reload: "reload",
    quit: "quit",
    clear_filter: "clear filter",
    apply_filter: "apply filter",
    select: "select",
    filter_hosts: "filter hosts",
    new_connection: "New connection",
    new_row: "+ New connection",
    new_description: "Add a host or connect once without saving.",
    no_matching_hosts: "No matching hosts",
    read_only: "read-only",
    read_only_file: "Read-only file",
    enter_runs: "enter runs",
    update_available: "update available ",
    nickname: "Nickname",
    user: "User",
    address: "Address",
    port: "Port",
    key_file: "Key file",
    key_login: "Key login",
    password: "Password",
    source: "Source",
    next: "next",
    back: "back",
    clear_field: "clear field",
    pick_key: "pick key",
    browse: "browse",
    toggle: "toggle",
    save_and_connect: "save & connect",
    review_changes: "review changes",
    install_public_key: "Install the public key on the server",
    password_once: "asks for the server password once",
    disable_password_prompt: "Disable password prompt (recommended)",
    check_key_login: "make sure key login already works",
    optional_save: "optional · fill in to save this host",
    required: "required",
    optional: "optional",
    on_enter: "On enter",
    save_step: "Save",
    key_step: "Key",
    run_step: "Run",
    connect_without_saving: "Leave Nickname empty to connect once without saving.",
    public_key_placeholder: "<public key>",
    cancel: "cancel",
    choose: "choose",
    back_to_list: "back to list",
    scroll: "scroll",
    save: "save",
    back_to_form: "back to form",
    cancel_choice: "Cancel",
    delete_choice: "Delete",
    forget_choice: "Forget",
    key_picker: "Select a key file",
    use_key: "use this key",
    import_history: "Import from shell history",
    import_guidance: "Pick one to open the new connection form with these values.",
    fill_form: "fill form",
    close: "close",
    connect_anyway: "connect anyway",
    ok: "ok",
    mismatch_title: "Saved settings differ from effective SSH settings",
    installed_via: "Installed via",
    command: "Command",
    download: "Download",
    update_title: "Update sshtui",
    update_and_exit: "update and exit",
    reloaded: "Reloaded",
    no_history_suggestions: "No new ssh destinations in shell history",
    needs_key_file: "Key login needs a key file",
    no_changes: "No changes",
    no_backup: "none",
    server_password_prompt: "sshtui: the server may ask for your password",
    key_install_finished: "sshtui: key install finished",
    key_install_failed: "sshtui: key install failed",
    return_after_install: "Press Enter to return to sshtui.",
};

const KO: Text = Text {
    connect: "접속",
    new: "새 접속",
    edit: "편집",
    delete: "삭제",
    forget_known_host: "known_host 삭제",
    forget: "삭제",
    filter: "필터",
    reload: "다시 읽기",
    quit: "종료",
    clear_filter: "필터 해제",
    apply_filter: "필터 적용",
    select: "선택",
    filter_hosts: "호스트 필터",
    new_connection: "새 접속",
    new_row: "+ 새 접속",
    new_description: "호스트 추가 또는 저장 없이 1회 접속",
    no_matching_hosts: "일치하는 호스트 없음",
    read_only: "읽기 전용",
    read_only_file: "읽기 전용 파일",
    enter_runs: "Enter 실행 명령",
    update_available: "업데이트 있음 ",
    nickname: "닉네임",
    user: "사용자",
    address: "주소",
    port: "포트",
    key_file: "키 파일",
    key_login: "키 로그인",
    password: "비밀번호",
    source: "출처",
    next: "다음",
    back: "뒤로",
    clear_field: "지우기",
    pick_key: "키 선택",
    browse: "파일 찾기",
    toggle: "전환",
    save_and_connect: "저장 후 접속",
    review_changes: "변경 확인",
    install_public_key: "서버에 공개 키 설치",
    password_once: "서버 비밀번호 1회 입력",
    disable_password_prompt: "비밀번호 입력 비활성화(권장)",
    check_key_login: "키 로그인 동작 확인 필요",
    optional_save: "선택, 입력하면 이 호스트를 저장",
    required: "필수",
    optional: "선택",
    on_enter: "Enter 실행 내용",
    save_step: "저장",
    key_step: "키",
    run_step: "실행",
    connect_without_saving: "닉네임을 비우면 저장 없이 1회 접속",
    public_key_placeholder: "<공개 키>",
    cancel: "취소",
    choose: "선택 이동",
    back_to_list: "목록으로",
    scroll: "스크롤",
    save: "저장",
    back_to_form: "폼으로",
    cancel_choice: "취소",
    delete_choice: "삭제",
    forget_choice: "삭제",
    key_picker: "키 파일 선택",
    use_key: "이 키 사용",
    import_history: "셸 히스토리에서 가져오기",
    import_guidance: "후보 선택 시 새 접속 폼에 입력",
    fill_form: "폼 채우기",
    close: "닫기",
    connect_anyway: "그대로 접속",
    ok: "확인",
    mismatch_title: "저장 설정과 실제 SSH 설정 불일치",
    installed_via: "설치 방식",
    command: "명령",
    download: "다운로드",
    update_title: "sshtui 업데이트",
    update_and_exit: "업데이트 후 종료",
    reloaded: "다시 읽음",
    no_history_suggestions: "셸 히스토리에 새 SSH 접속 대상 없음",
    needs_key_file: "키 로그인에 키 파일 필요",
    no_changes: "변경 없음",
    no_backup: "없음",
    server_password_prompt: "sshtui: 서버가 비밀번호를 요청할 수 있음",
    key_install_finished: "sshtui: 키 설치 완료",
    key_install_failed: "sshtui: 키 설치 실패",
    return_after_install: "Enter로 sshtui 복귀",
};

impl Text {
    /// Returns the form's label for a field.
    pub(crate) fn caption(&self, field: FormField) -> &'static str {
        match field {
            FormField::Nickname => self.nickname,
            FormField::User => self.user,
            FormField::Address => self.address,
            FormField::Port => self.port,
            FormField::KeyFile => self.key_file,
            FormField::InstallKey => self.key_login,
            FormField::DisablePassword => self.password,
        }
    }

    fn validation_field(&self, field: Field) -> &'static str {
        match field {
            Field::Nickname => self.nickname,
            Field::User => self.user,
            Field::Address => self.address,
            Field::Port => self.port,
            Field::KeyFile => self.key_file,
        }
    }
}

impl Lang {
    /// Returns the fixed text for this language.
    pub(crate) fn text(self) -> &'static Text {
        match self {
            Self::En => &EN,
            Self::Ko => &KO,
        }
    }

    /// Formats the total host count and the filtered count when present.
    pub(crate) fn hosts(self, total: usize, matched: Option<usize>) -> String {
        match (self, matched) {
            (Self::En, None) => format!("{total} hosts"),
            (Self::En, Some(matched)) => format!("{matched} of {total} hosts"),
            (Self::Ko, None) => format!("호스트 {total}개"),
            (Self::Ko, Some(matched)) => format!("호스트 {total}개 중 {matched}개"),
        }
    }

    /// Formats the count of history suggestions.
    pub(crate) fn suggestions(self, count: usize) -> String {
        match self {
            Self::En if count == 1 => "1 suggestion".into(),
            Self::En => format!("{count} suggestions"),
            Self::Ko => format!("후보 {count}개"),
        }
    }

    /// Formats the heading for a host edit.
    pub(crate) fn edit_title(self, name: &str) -> String {
        match self {
            Self::En => format!("Edit {name}"),
            Self::Ko => format!("{name} 편집"),
        }
    }

    /// Formats the heading for a pending change.
    pub(crate) fn confirm_title(self, kind: ConfirmKind, name: &str) -> String {
        match (self, kind) {
            (Self::En, ConfirmKind::Save) => format!("Save {name}"),
            (Self::En, ConfirmKind::Delete) => format!("Delete {name}"),
            (Self::En, ConfirmKind::Forget) => format!("Forget host key for {name}"),
            (Self::Ko, ConfirmKind::Save) => format!("{name} 저장"),
            (Self::Ko, ConfirmKind::Delete) => format!("{name} 삭제"),
            (Self::Ko, ConfirmKind::Forget) => format!("{name} 호스트 키 삭제"),
        }
    }

    /// Formats the prompt for deleting a saved host.
    pub(crate) fn delete_question(self, name: &str, path: &str) -> String {
        match self {
            Self::En => format!("Delete {name} from {path}?"),
            Self::Ko => format!("{path}에서 {name} 삭제?"),
        }
    }

    /// Formats the prompt for deleting known_hosts entries.
    pub(crate) fn forget_question(self, count: usize, name: &str) -> String {
        match self {
            Self::En if count == 1 => format!("Remove 1 known_hosts entry for {name}?"),
            Self::En => format!("Remove {count} known_hosts entries for {name}?"),
            Self::Ko => format!("{name} known_hosts 항목 {count}개 삭제?"),
        }
    }

    /// Formats the number of changed lines.
    pub(crate) fn lines_changed(self, count: usize) -> String {
        match self {
            Self::En if count == 1 => "1 line will change".into(),
            Self::En => format!("{count} lines will change"),
            Self::Ko => format!("{count}줄 변경 예정"),
        }
    }

    /// Formats the number of known_hosts entries to be deleted.
    pub(crate) fn entries_to_remove(self, count: usize, name: &str) -> String {
        match self {
            Self::En if count == 1 => format!("1 entry for {name} will be removed"),
            Self::En => format!("{count} entries for {name} will be removed"),
            Self::Ko => format!("{name} 항목 {count}개 삭제 예정"),
        }
    }

    /// Formats the backup notice for a pending change.
    pub(crate) fn current_file_backup(self, backup: &str) -> String {
        match self {
            Self::En => format!("The current file is kept as {backup}"),
            Self::Ko => format!("현재 파일 백업: {backup}"),
        }
    }

    /// Formats the key count for the picker heading.
    pub(crate) fn keys_in_directory(self, count: usize, directory: &str) -> String {
        match self {
            Self::En if count == 1 => format!("1 key in {directory}"),
            Self::En => format!("{count} keys in {directory}"),
            Self::Ko => format!("{directory}에 키 {count}개"),
        }
    }

    /// Formats the result count for the history picker.
    pub(crate) fn import_found(self, count: usize) -> String {
        match self {
            Self::En => format!("{count} found"),
            Self::Ko => format!("{count}개 발견"),
        }
    }

    /// Formats an inherited value shown in an empty input.
    pub(crate) fn inherited(self, value: &str) -> String {
        match self {
            Self::En => format!("inherited: {value}"),
            Self::Ko => format!("상속: {value}"),
        }
    }

    /// Formats the count of keys below the visible list.
    pub(crate) fn more_keys(self, count: usize) -> String {
        match self {
            Self::En => format!("… {count} more"),
            Self::Ko => format!("… 키 {count}개 더 있음"),
        }
    }

    /// Formats the action required to proceed past a validation error.
    pub(crate) fn continue_form(self, field: FormField, error: &ValidationError) -> String {
        let field = self.text().caption(field);
        match (self, error) {
            (Self::En, ValidationError::Empty { .. }) => format!("Fill in {field} to continue"),
            (Self::En, _) => format!("Fix {field} to continue"),
            (Self::Ko, ValidationError::Empty { .. }) => format!("{field} 입력 필요"),
            (Self::Ko, _) => format!("{field} 수정 필요"),
        }
    }

    /// Formats the next step before reviewing an edited configuration.
    pub(crate) fn review_source(self, source: &str) -> String {
        match self {
            Self::En => format!("Review changes to {source}"),
            Self::Ko => format!("{source} 변경 확인"),
        }
    }

    /// Formats the alias and destination for a new saved host.
    pub(crate) fn save_destination(self, nickname: &str, path: &str) -> String {
        match self {
            Self::En => format!("{nickname} to {path}"),
            Self::Ko => format!("{nickname} → {path}"),
        }
    }

    /// Formats the hint for enabling a save from the new-connection form.
    pub(crate) fn fill_nickname(self, path: &str) -> String {
        match self {
            Self::En => format!("Fill in Nickname to save this host to {path}."),
            Self::Ko => format!("닉네임 입력 시 {path}에 호스트 저장"),
        }
    }

    /// Formats the key installation step in the form.
    pub(crate) fn install_on_server(self, path: &str) -> String {
        match self {
            Self::En => format!("install {path} on the server"),
            Self::Ko => format!("서버에 {path} 설치"),
        }
    }

    /// Formats the notice about editing a Host block with multiple patterns.
    pub(crate) fn shared_options(self, siblings: &str, name: &str) -> String {
        match self {
            Self::En => {
                format!("Option changes also apply to {siblings}. Renaming affects only {name}.")
            }
            Self::Ko => format!("옵션 변경은 {siblings}에도 적용, 이름 변경은 {name}에만 적용"),
        }
    }

    /// Labels an error from the effective settings lookup.
    pub(crate) fn effective_settings(self, error: &str) -> String {
        match self {
            Self::En => format!("Effective settings: {error}"),
            Self::Ko => format!("실제 설정: {error}"),
        }
    }

    /// Formats the difference between a saved value and its effective value.
    pub(crate) fn mismatch(self, mismatch: &Mismatch) -> String {
        match self {
            Self::En => mismatch.to_string(),
            Self::Ko => {
                let text = self.text();
                let label = match mismatch.key {
                    OptionKey::HostName => text.address,
                    OptionKey::User => text.user,
                    OptionKey::Port => text.port,
                    OptionKey::IdentityFile => text.key_file,
                    OptionKey::PasswordAuthentication => text.password,
                };
                format!(
                    "{label}: 저장 값 {:?}, SSH 사용 값 {:?}",
                    mismatch.expected, mismatch.actual
                )
            }
        }
    }

    /// Formats the installation method for the update overlay.
    pub(crate) fn install_method(self, method: &InstallMethod) -> String {
        match self {
            Self::En => method.describe(),
            Self::Ko => match method {
                InstallMethod::Brew | InstallMethod::Cargo => method.describe(),
                InstallMethod::Script { bin_dir } => format!("{}의 install.sh", bin_dir.display()),
                InstallMethod::Deb { .. } => "Debian 패키지 (.deb)".into(),
                InstallMethod::Rpm { .. } => "RPM 패키지".into(),
                InstallMethod::Unknown => "알 수 없음(수동 설치)".into(),
            },
        }
    }

    /// Formats a validation error for the status line or preview.
    pub(crate) fn validation_error(self, error: &ValidationError) -> String {
        match self {
            Self::En => error.to_string(),
            Self::Ko => {
                let caption = |field| self.text().validation_field(field);
                match error {
                    ValidationError::Empty { field } => format!("{} 필수", caption(*field)),
                    ValidationError::LeadingDash { field } => {
                        format!("{} 첫 글자 '-' 사용 불가", caption(*field))
                    }
                    ValidationError::ForbiddenChar { field, ch } => {
                        format!("{}에 {ch:?} 사용 불가", caption(*field))
                    }
                    ValidationError::Whitespace { field } => {
                        format!("{}에 공백 사용 불가", caption(*field))
                    }
                    ValidationError::ControlChar { field } => {
                        format!("{}에 제어 문자 사용 불가", caption(*field))
                    }
                    ValidationError::InvalidPort => "포트는 1-65535 범위의 숫자만 허용".into(),
                    ValidationError::DuplicateNickname(name) => {
                        format!("이미 사용 중인 닉네임 {name:?}")
                    }
                }
            }
        }
    }

    /// Formats a validation error beneath its input.
    pub(crate) fn inline_validation_error(self, error: &ValidationError) -> String {
        if self == Self::En && *error == ValidationError::InvalidPort {
            return "Port must be a number from 1 to 65535".into();
        }
        let mut text = self.validation_error(error);
        if self == Self::En
            && let Some(first) = text.get_mut(..1)
        {
            first.make_ascii_uppercase();
        }
        text
    }

    /// Formats a key installation failure with an SSH exit code.
    pub(crate) fn key_install_exit(self, label: &str, code: i32) -> String {
        match self {
            Self::En => format!("Key install failed for {label} (ssh exit code {code})"),
            Self::Ko => format!("{label} 키 설치 실패(ssh 종료 코드 {code})"),
        }
    }

    /// Formats a key installation failure with its underlying error.
    pub(crate) fn key_install_error(self, label: &str, error: &str) -> String {
        match self {
            Self::En => format!("Key install failed for {label}: {error}"),
            Self::Ko => format!("{label} 키 설치 실패: {error}"),
        }
    }

    /// Formats the status after installing a public key.
    pub(crate) fn key_installed(self, label: &str) -> String {
        match self {
            Self::En => format!("Installed the public key on {label}"),
            Self::Ko => format!("{label}에 공개 키 설치 완료"),
        }
    }

    /// Formats the terminal announcement before installing a public key.
    pub(crate) fn key_install_started(self, label: &str) -> String {
        match self {
            Self::En => format!("sshtui: installing the public key on {label}"),
            Self::Ko => format!("sshtui: {label}에 공개 키 설치 중"),
        }
    }

    /// Adds a count to the first configuration warning.
    pub(crate) fn warning_count(self, warning: &ReadWarning, count: usize) -> String {
        let warning = match warning {
            ReadWarning::Unreadable { path, error } => {
                format!("Cannot read included file {}: {error}", path.display())
            }
            ReadWarning::UnsupportedInclude {
                file,
                line_number,
                pattern,
            } => format!(
                "{}:{line_number}: unsupported Include pattern {pattern:?}",
                file.display()
            ),
            ReadWarning::InvalidGlob {
                file,
                line_number,
                pattern,
            } => format!(
                "{}:{line_number}: invalid Include pattern {pattern:?}",
                file.display()
            ),
            ReadWarning::DepthExceeded { path } => {
                format!("Include nesting is too deep at {}", path.display())
            }
        };
        match self {
            Self::En => format!("{warning} ({count} warnings total)"),
            Self::Ko => format!("{warning} (경고 총 {count}개)"),
        }
    }

    /// Formats the warning for a read-only configuration path.
    pub(crate) fn path_read_only(self, path: &Path) -> String {
        let path = path.display();
        match self {
            Self::En => format!("{path} is read-only"),
            Self::Ko => format!("{path}: 읽기 전용"),
        }
    }

    /// Formats the error for a host whose configuration file cannot be found.
    pub(crate) fn missing_config(self, name: &str) -> String {
        match self {
            Self::En => format!("Cannot find the config file for {name}"),
            Self::Ko => format!("{name}의 설정 파일을 찾을 수 없음"),
        }
    }

    /// Formats a host lookup failure with its underlying error.
    pub(crate) fn cannot_resolve(self, alias: &str, error: &str) -> String {
        match self {
            Self::En => format!("Cannot resolve {alias}: {error}"),
            Self::Ko => format!("{alias} 확인 실패: {error}"),
        }
    }

    /// Formats the status when there is no known_hosts entry to delete.
    pub(crate) fn no_known_host_entry(self, name: &str, path: &str) -> String {
        match self {
            Self::En => format!("No entry for {name} in {path}"),
            Self::Ko => format!("{path}에 {name} 항목 없음"),
        }
    }

    /// Formats the warning for a directory with no private keys.
    pub(crate) fn no_private_keys(self, path: &Path) -> String {
        let path = path.display();
        match self {
            Self::En => format!("No private keys found in {path}"),
            Self::Ko => format!("{path}에 개인 키 없음"),
        }
    }

    /// Formats the retry instructions for a new host after an external write.
    pub(crate) fn stale_add(self, path: &Path) -> String {
        let path = path.display();
        match self {
            Self::En => {
                format!("{path} was changed by another program; reloaded, press Enter to retry")
            }
            Self::Ko => format!("{path} 외부 변경 감지, 다시 읽음, Enter로 재시도"),
        }
    }

    /// Formats the retry instructions for an expired edit or deletion.
    pub(crate) fn stale_confirm(self, path: &Path) -> String {
        let path = path.display();
        match self {
            Self::En => format!("{path} was changed by another program; reloaded, try again"),
            Self::Ko => format!("{path} 외부 변경 감지, 다시 읽음, 재시도 필요"),
        }
    }

    /// Formats the retry instructions for an expired known_hosts change.
    pub(crate) fn stale_forget(self, path: &Path) -> String {
        let path = path.display();
        match self {
            Self::En => format!("{path} was changed by another program; press x to try again"),
            Self::Ko => format!("{path} 외부 변경 감지, x로 재시도"),
        }
    }

    fn backup_description(self, backup: Option<&Path>) -> String {
        backup.map_or_else(
            || self.text().no_backup.into(),
            |path| path.display().to_string(),
        )
    }

    /// Formats success for an edit or deletion and reports its backup.
    pub(crate) fn confirmed(self, edited: bool, alias: &str, backup: Option<&Path>) -> String {
        let backup = self.backup_description(backup);
        match (self, edited) {
            (Self::En, true) => format!("Saved {alias} (backup: {backup})"),
            (Self::En, false) => format!("Deleted {alias} (backup: {backup})"),
            (Self::Ko, true) => format!("{alias} 저장 완료 (백업: {backup})"),
            (Self::Ko, false) => format!("{alias} 삭제 완료 (백업: {backup})"),
        }
    }

    /// Formats success for a known_hosts change and reports its backup.
    pub(crate) fn removed_entries(self, count: usize, name: &str, backup: Option<&Path>) -> String {
        let backup = self.backup_description(backup);
        match self {
            Self::En if count == 1 => {
                format!("Removed 1 known_hosts entry for {name} (backup: {backup})")
            }
            Self::En => {
                format!("Removed {count} known_hosts entries for {name} (backup: {backup})")
            }
            Self::Ko => format!("{name} known_hosts 항목 {count}개 삭제 완료 (백업: {backup})"),
        }
    }
}

// Backend fallback errors are passed through unchanged in both languages.
pub(crate) const PUBLIC_KEY_LOOKUP_UNAVAILABLE: &str = "public key lookup is unavailable";
pub(crate) const KEY_INSTALL_UNAVAILABLE: &str = "key installation is unavailable";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_the_language_part_of_the_locale() {
        for (locales, expected) in [
            (
                &["ko", "ko_KR.UTF-8", "ko-KR", " KO_kr ", "ko@variant"][..],
                Some(Lang::Ko),
            ),
            (
                &["en_US.UTF-8", "en_KR.UTF-8", "ja_JP", "kok"][..],
                Some(Lang::En),
            ),
            (
                &["", "  ", "C", "c.Utf-8", "POSIX", "posix@variant"][..],
                None,
            ),
        ] {
            for locale in locales {
                assert_eq!(lang_of_locale(locale), expected, "{locale:?}");
            }
        }
    }

    #[test]
    fn detects_language_in_priority_order_and_skips_unspecified_locales() {
        type Env = &'static [(&'static str, &'static str)];
        let cases: &[(Env, Option<&str>, Lang)] = &[
            (&[(LANG_ENV, "kO"), ("LC_ALL", "en")], Some("en"), Lang::Ko),
            (&[(LANG_ENV, "EN"), ("LC_ALL", "ko")], Some("ko"), Lang::En),
            (
                &[
                    (LANG_ENV, "ko-KR"),
                    ("LC_ALL", "en"),
                    ("LC_MESSAGES", "ko"),
                    ("LANG", "ko"),
                ],
                Some("ko"),
                Lang::En,
            ),
            (
                &[("LC_ALL", "ko"), ("LC_MESSAGES", "en"), ("LANG", "en")],
                Some("en"),
                Lang::Ko,
            ),
            (
                &[
                    ("LC_ALL", "c.UTF-8"),
                    ("LC_MESSAGES", "ja_JP"),
                    ("LANG", "ko"),
                ],
                Some("ko"),
                Lang::En,
            ),
            (
                &[
                    ("LC_ALL", "C"),
                    ("LC_MESSAGES", " "),
                    ("LANG", "ko_KR.UTF-8"),
                ],
                Some("en"),
                Lang::Ko,
            ),
            (&[("LANG", "en_KR")], Some("ko-KR"), Lang::En),
            (&[("LANG", "POSIX")], Some("KO-kr"), Lang::Ko),
            (&[], Some("ko-KR"), Lang::Ko),
            (&[], Some("ja_JP"), Lang::En),
            (&[], Some("C.UTF-8"), Lang::En),
            (&[], None, Lang::En),
        ];
        for &(values, system, expected) in cases {
            let env = |name: &str| {
                values
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| OsString::from(*value))
            };
            assert_eq!(detect(&env, system), expected, "{values:?}, {system:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn skips_non_unicode_environment_values() {
        use std::os::unix::ffi::OsStringExt;

        let env = |name: &str| match name {
            LANG_ENV | "LC_ALL" => Some(OsString::from_vec(vec![0xff])),
            "LC_MESSAGES" => Some("ko-KR".into()),
            _ => None,
        };
        assert_eq!(detect(&env, Some("en")), Lang::Ko);
    }

    #[test]
    fn preserves_representative_english_text_and_formats_values_in_each_language() {
        let en = Lang::En;
        let ko = Lang::Ko;
        assert_eq!(en.text().new_row, "+ New connection");
        assert_eq!(
            en.text().optional_save,
            "optional · fill in to save this host"
        );
        assert_eq!(en.text().forget_known_host, "forget(known_host)");
        assert_eq!(
            en.text().server_password_prompt,
            "sshtui: the server may ask for your password"
        );
        assert_eq!(en.hosts(4, None), "4 hosts");
        assert_eq!(ko.hosts(4, None), "호스트 4개");
        assert_eq!(en.hosts(4, Some(2)), "2 of 4 hosts");
        assert_eq!(ko.hosts(4, Some(2)), "호스트 4개 중 2개");
        assert_eq!(en.suggestions(1), "1 suggestion");
        assert_eq!(en.suggestions(2), "2 suggestions");
        assert_eq!(ko.suggestions(1), "후보 1개");
        assert_eq!(ko.suggestions(2), "후보 2개");
        assert_eq!(en.lines_changed(1), "1 line will change");
        assert_eq!(ko.lines_changed(1), "1줄 변경 예정");
        assert_eq!(
            en.entries_to_remove(1, "web"),
            "1 entry for web will be removed"
        );
        assert_eq!(ko.entries_to_remove(2, "web"), "web 항목 2개 삭제 예정");
        assert_eq!(
            en.current_file_backup("backup"),
            "The current file is kept as backup"
        );
        assert_eq!(ko.current_file_backup("backup"), "현재 파일 백업: backup");
        assert_eq!(
            en.validation_error(&ValidationError::InvalidPort),
            "port must be a number between 1 and 65535"
        );
        assert_eq!(
            en.inline_validation_error(&ValidationError::InvalidPort),
            "Port must be a number from 1 to 65535"
        );
    }
}

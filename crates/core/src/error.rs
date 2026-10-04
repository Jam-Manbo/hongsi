use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("학교 서버에 연결하지 못했어요.")]
    Network(#[from] reqwest::Error),
    #[error("{0}")]
    LoginRejected(String),
    #[error("학교 로그인 세션이 만료됐어요.")]
    SessionExpired,
    #[error("클래스룸 로그인 토큰이 만료됐어요.")]
    ClassroomTokenExpired,
    #[error("학교 페이지의 정보를 읽지 못했어요 ({0})")]
    Parse(String),
    #[error("{0}")]
    Upstream(String),
    #[error("{0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;

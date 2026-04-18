use std::io;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SpoutError {
    #[error("invalid utf-8 in {0}: {1:?}")]
    InvalidUtf8(&'static str, std::ffi::OsString),

    #[error("failed to resolve config dir")]
    NoConfigDir,

    #[error("config exists at {0} -- use -G to overwrite")]
    ConfigExists(std::path::PathBuf),

    #[error("config not found at {0} -- run: spout -g")]
    ConfigNotFound(std::path::PathBuf),

    #[error("insecure config permissions -- run: chmod 600 {0}")]
    InsecureConfig(std::path::PathBuf),

    #[error("parse error: {0}")]
    ParseError(String),

    #[error("rng error: {0}")]
    RngError(#[source] getrandom::Error),

    #[error("dangerous characters in filename: {0}")]
    DangerousFilename(String),

    #[error("invalid url: {0}")]
    InvalidUrl(#[from] url::ParseError),

    #[error("unsupported scheme: {0}")]
    UnsupportedScheme(String),

    #[error("no host in url")]
    NoHost,

    #[error("no port in url")]
    NoPort,

    #[error("dns resolution failed: {0}")]
    DnsResolution(#[source] io::Error),

    #[error("dns resolution timed out")]
    DnsTimeout,

    #[error("no addresses resolved for host")]
    NoAddresses,

    #[error("url resolves to a private ip address")]
    PrivateIp,

    #[error("response is not valid json: {0}")]
    InvalidJson(#[from] serde_json::Error),

    #[error("key '{0}' not found in response")]
    KeyNotFound(String),

    #[error("response path '{0}' is not a string")]
    NotAString(String),

    #[error("response body is too large to be a valid url")]
    ResponseTooLarge,

    #[error("response exceeds {0} MB limit")]
    ResponseTooLargeLimit(u64),

    #[error("response value is not a valid url: {0}")]
    ResponseInvalidUrl(#[source] url::ParseError),

    #[error("unexpected scheme in response url: {0}")]
    ResponseUnexpectedScheme(String),

    #[error("fields are not supported for binary format uploads")]
    FieldsInBinaryFormat,

    #[error("clipboard binary must be a name, not a path")]
    ClipboardPathNotAllowed,

    #[error("clipboard binary '{0}' is not allowed")]
    ClipboardBinaryNotAllowed(String),

    #[error("failed to spawn clipboard binary '{0}': {1}")]
    ClipboardSpawn(String, #[source] io::Error),

    #[error("input exceeds {0} MB limit")]
    InputTooLarge(u64),

    #[error("failed to strip metadata from {0}")]
    SanitizeFailed(&'static str),

    #[error("unsupported http method: {0}")]
    UnsupportedMethod(String),

    #[error("unsupported upload format: {0}")]
    UnsupportedFormat(String),

    #[error("no input data -- usage: <cmd> | spout [profile]")]
    NoInputData,

    #[error("no profile named '{0}'")]
    ProfileNotFound(String),

    #[error("failed to read response: {0}")]
    ResponseReadError(#[source] io::Error),

    #[error("upload failed ({0}) -- {1}")]
    UploadFailed(reqwest::StatusCode, String),

    #[error(transparent)]
    Io(#[from] io::Error),

    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    #[error(transparent)]
    Lexopt(#[from] lexopt::Error),
}

pub type Result<T, E = SpoutError> = std::result::Result<T, E>;

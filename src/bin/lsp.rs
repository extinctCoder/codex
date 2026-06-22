//! Holocron LSP server: parses the open YAML buffer on every change and
//! publishes Holocron diagnostics. Diagnostics carry source spans we already
//! attach in L1–L3, converted from byte offsets to LSP `Range`s here.
//!
//! Wire it into a JetBrains IDE via the LSP4IJ plugin: point the language
//! server command at the compiled `holocron-lsp` binary; file pattern
//! `*.yml`/`*.yaml` works (we degrade silently on non-Holocron YAML).

use holocron::{compile, HolocronError, Span};
use tokio::io::{stdin, stdout};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{Diagnostic, OneOf};
use tower_lsp::lsp_types::{
    DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, InitializeParams, InitializeResult, InitializedParams, MessageType,
    Position, Range, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
    TextDocumentSyncKind, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

const SERVER_NAME: &str = "holocron-lsp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Source string we tag each diagnostic with — JetBrains shows this as the
/// "produced by" label next to the underline.
const DIAGNOSTIC_SOURCE: &str = "holocron";

struct Backend {
    client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: SERVER_NAME.to_string(),
                version: Some(SERVER_VERSION.to_string()),
            }),
            capabilities: ServerCapabilities {
                // FULL sync: the editor sends the entire document on every change.
                // For schema-sized files this is cheap and far simpler than
                // tracking incremental edits.
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                definition_provider: Some(OneOf::Left(false)),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, format!("{SERVER_NAME} ready"))
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "didOpen: {} (languageId={}, {} bytes)",
                    params.text_document.uri,
                    params.text_document.language_id,
                    params.text_document.text.len(),
                ),
            )
            .await;
        self.publish(params.text_document.uri, &params.text_document.text)
            .await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        // With FULL sync there is exactly one change, carrying the new buffer.
        let Some(change) = params.content_changes.pop() else {
            return;
        };
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "didChange: {} ({} bytes)",
                    params.text_document.uri,
                    change.text.len()
                ),
            )
            .await;
        self.publish(params.text_document.uri, &change.text).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("didClose: {}", params.text_document.uri),
            )
            .await;
        // Clear diagnostics when the buffer closes so stale squiggles don't linger.
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

impl Backend {
    async fn publish(&self, uri: Url, source: &str) {
        let diagnostics = match compile(source) {
            Ok(_) => Vec::new(),
            Err(error) => vec![diagnostic_for(&error, source)],
        };
        self.client
            .log_message(
                MessageType::INFO,
                format!("publish: {} ({} diagnostic(s))", uri, diagnostics.len()),
            )
            .await;
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

/// Convert a single [`HolocronError`] into an LSP [`Diagnostic`]. Errors
/// without a span (the L4 query-checker variants) are reported at the start
/// of the file so they still surface in the Problems panel.
fn diagnostic_for(error: &HolocronError, source: &str) -> Diagnostic {
    let range = error
        .span()
        .map(|span| span_to_range(span, source))
        .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 1)));
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some(DIAGNOSTIC_SOURCE.to_string()),
        message: error.to_string(),
        ..Default::default()
    }
}

fn span_to_range(span: Span, source: &str) -> Range {
    Range {
        start: offset_to_position(span.start, source),
        end: offset_to_position(span.end, source),
    }
}

/// Byte offset → LSP `Position` (0-indexed line + character).
///
/// LSP defaults to UTF-16 code-unit columns, but for the ASCII-dominant
/// schema YAML we work with, byte counts match. A pure-UTF-8 client would
/// also be exact. Non-ASCII keys/values may render the underline off by a
/// few code units — fix when it bites by switching to a `LineIndex` that
/// counts UTF-16 units per line.
fn offset_to_position(offset: usize, source: &str) -> Position {
    let offset = offset.min(source.len());
    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    for (index, byte) in source.bytes().enumerate() {
        if index >= offset {
            break;
        }
        if byte == b'\n' {
            line += 1;
            line_start = index + 1;
        }
    }
    let character = (offset - line_start) as u32;
    Position { line, character }
}

#[tokio::main]
async fn main() {
    let (service, socket) = LspService::new(|client| Backend { client });
    Server::new(stdin(), stdout(), socket).serve(service).await;
}

use anyhow::Context as _;
use codex_agent_identity::AgentIdentityKey;
use codex_agent_identity::AgentTaskAuthorizationTarget;
use codex_agent_identity::authorization_header_for_agent_task;
use codex_agent_identity::normalize_chatgpt_base_url;
use codex_agent_identity::register_agent_task_blocking;
use once_cell::sync::OnceCell;
use serde::Deserialize;

use super::storage::AgentIdentityAuthRecord;

const DEFAULT_CHATGPT_BACKEND_BASE_URL: &str = "https://chatgpt.com/backend-api";

#[derive(Debug)]
pub struct AgentIdentityAuth {
    record: AgentIdentityAuthRecord,
    runtime: OnceCell<AgentIdentityRuntimeState>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentIdentityRuntimeState {
    pub process_task_id: String,
    pub account_info: AgentIdentityAccountInfo,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentIdentityAccountInfo {
    pub account_id: Option<String>,
    pub chatgpt_user_id: Option<String>,
    pub email: Option<String>,
    pub is_fedramp_account: bool,
}

#[derive(Deserialize)]
struct AccountInfoResponse {
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    chatgpt_account_id: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    chatgpt_user_id: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    chatgpt_account_is_fedramp: Option<bool>,
}

impl Clone for AgentIdentityAuth {
    fn clone(&self) -> Self {
        let runtime = OnceCell::new();
        if let Some(state) = self.runtime.get() {
            let _ = runtime.set(state.clone());
        }
        Self {
            record: self.record.clone(),
            runtime,
        }
    }
}

impl AgentIdentityAuth {
    pub fn new(record: AgentIdentityAuthRecord) -> Self {
        Self {
            record,
            runtime: OnceCell::new(),
        }
    }

    pub fn record(&self) -> &AgentIdentityAuthRecord {
        &self.record
    }

    pub fn runtime(&self) -> &OnceCell<AgentIdentityRuntimeState> {
        &self.runtime
    }

    pub fn initialize_runtime_blocking(
        &self,
        chatgpt_base_url: Option<String>,
    ) -> std::io::Result<()> {
        self.runtime
            .get_or_try_init(|| {
                let base_url = normalize_chatgpt_base_url(
                    chatgpt_base_url
                        .as_deref()
                        .unwrap_or(DEFAULT_CHATGPT_BACKEND_BASE_URL),
                );
                let process_task_id = register_agent_task_blocking(&base_url, self.key())
                    .map_err(std::io::Error::other)?;
                let account_info =
                    fetch_account_info_blocking(&base_url, &self.record, &process_task_id)?;
                Ok(AgentIdentityRuntimeState {
                    process_task_id,
                    account_info,
                })
            })
            .map(|_| ())
    }

    pub fn authorization_header_value(&self) -> std::io::Result<String> {
        let runtime = self.runtime.get().ok_or_else(|| {
            std::io::Error::other("agent identity runtime state is not initialized")
        })?;
        authorization_header_for_agent_task(
            self.key(),
            AgentTaskAuthorizationTarget {
                agent_runtime_id: &self.record.agent_runtime_id,
                task_id: &runtime.process_task_id,
            },
        )
        .map_err(std::io::Error::other)
    }

    pub fn account_info(&self) -> Option<&AgentIdentityAccountInfo> {
        self.runtime.get().map(|state| &state.account_info)
    }

    fn key(&self) -> AgentIdentityKey<'_> {
        AgentIdentityKey {
            agent_runtime_id: &self.record.agent_runtime_id,
            private_key_pkcs8_base64: &self.record.agent_private_key,
        }
    }
}

fn fetch_account_info_blocking(
    chatgpt_base_url: &str,
    record: &AgentIdentityAuthRecord,
    process_task_id: &str,
) -> std::io::Result<AgentIdentityAccountInfo> {
    let authorization_header = authorization_header_for_agent_task(
        AgentIdentityKey {
            agent_runtime_id: &record.agent_runtime_id,
            private_key_pkcs8_base64: &record.agent_private_key,
        },
        AgentTaskAuthorizationTarget {
            agent_runtime_id: &record.agent_runtime_id,
            task_id: process_task_id,
        },
    )
    .map_err(std::io::Error::other)?;

    let response: AccountInfoResponse = reqwest::blocking::Client::new()
        .get(format!("{}/me", chatgpt_base_url.trim_end_matches('/')))
        .header(reqwest::header::AUTHORIZATION, authorization_header)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(std::io::Error::other)?
        .json()
        .context("failed to decode agent identity account info")
        .map_err(std::io::Error::other)?;

    Ok(AgentIdentityAccountInfo {
        account_id: response.account_id.or(response.chatgpt_account_id),
        chatgpt_user_id: response.chatgpt_user_id.or(response.user_id),
        email: response.email,
        is_fedramp_account: response.chatgpt_account_is_fedramp.unwrap_or(false),
    })
}

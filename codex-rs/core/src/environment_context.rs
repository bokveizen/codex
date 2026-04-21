use crate::contextual_user_message::ENVIRONMENT_CONTEXT_FRAGMENT;
use crate::session::turn_context::TurnContext;
use crate::shell::Shell;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TurnContextItem;
use codex_protocol::protocol::TurnContextNetworkItem;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename = "environment_context", rename_all = "snake_case")]
pub(crate) struct EnvironmentContext {
    pub cwd: Option<PathBuf>,
    pub environments: Option<Vec<EnvironmentContextEnvironment>>,
    pub shell: Shell,
    pub current_date: Option<String>,
    pub timezone: Option<String>,
    pub network: Option<NetworkContext>,
    pub subagents: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct EnvironmentContextEnvironment {
    pub id: String,
    pub cwd: PathBuf,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct NetworkContext {
    allowed_domains: Vec<String>,
    denied_domains: Vec<String>,
}

impl EnvironmentContext {
    #[cfg(test)]
    pub fn new(
        cwd: Option<PathBuf>,
        shell: Shell,
        current_date: Option<String>,
        timezone: Option<String>,
        network: Option<NetworkContext>,
        subagents: Option<String>,
    ) -> Self {
        Self::new_with_environments(
            cwd,
            /*environments*/ None,
            shell,
            current_date,
            timezone,
            network,
            subagents,
        )
    }

    fn new_with_environments(
        cwd: Option<PathBuf>,
        environments: Option<Vec<EnvironmentContextEnvironment>>,
        shell: Shell,
        current_date: Option<String>,
        timezone: Option<String>,
        network: Option<NetworkContext>,
        subagents: Option<String>,
    ) -> Self {
        Self {
            cwd,
            environments,
            shell,
            current_date,
            timezone,
            network,
            subagents,
        }
    }

    /// Compares two environment contexts, ignoring the shell. Useful when
    /// comparing turn to turn, since the initial environment_context will
    /// include the shell, and then it is not configurable from turn to turn.
    pub fn equals_except_shell(&self, other: &EnvironmentContext) -> bool {
        let EnvironmentContext {
            cwd,
            environments,
            current_date,
            timezone,
            network,
            subagents,
            shell: _,
        } = other;
        self.cwd == *cwd
            && self.environments == *environments
            && self.current_date == *current_date
            && self.timezone == *timezone
            && self.network == *network
            && self.subagents == *subagents
    }

    pub fn diff_from_turn_context_item(
        before: &TurnContextItem,
        after: &TurnContext,
        shell: &Shell,
    ) -> Self {
        let before_network = Self::network_from_turn_context_item(before);
        let after_network = Self::network_from_turn_context(after);
        let cwd = if before.cwd.as_path() != after.cwd.as_path() {
            Some(after.cwd.to_path_buf())
        } else {
            None
        };
        let current_date = after.current_date.clone();
        let timezone = after.timezone.clone();
        let network = if before_network != after_network {
            after_network
        } else {
            before_network
        };
        EnvironmentContext::new_with_environments(
            cwd,
            Self::environments_for_diff(before, after),
            shell.clone(),
            current_date,
            timezone,
            network,
            /*subagents*/ None,
        )
    }

    pub fn from_turn_context(turn_context: &TurnContext, shell: &Shell) -> Self {
        let cwd = turn_context.default_environment_cwd();
        Self::new_with_environments(
            Some(cwd.to_path_buf()),
            Self::environments_from_turn_context(turn_context),
            shell.clone(),
            turn_context.current_date.clone(),
            turn_context.timezone.clone(),
            Self::network_from_turn_context(turn_context),
            /*subagents*/ None,
        )
    }

    pub fn from_turn_context_item(turn_context_item: &TurnContextItem, shell: &Shell) -> Self {
        Self::new_with_environments(
            Some(turn_context_item.cwd.clone()),
            Self::environments_from_turn_context_item(turn_context_item),
            shell.clone(),
            turn_context_item.current_date.clone(),
            turn_context_item.timezone.clone(),
            Self::network_from_turn_context_item(turn_context_item),
            /*subagents*/ None,
        )
    }

    pub fn with_subagents(mut self, subagents: String) -> Self {
        if !subagents.is_empty() {
            self.subagents = Some(subagents);
        }
        self
    }

    fn network_from_turn_context(turn_context: &TurnContext) -> Option<NetworkContext> {
        let network = turn_context
            .config
            .config_layer_stack
            .requirements()
            .network
            .as_ref()?;

        Some(NetworkContext {
            allowed_domains: network
                .domains
                .as_ref()
                .and_then(codex_config::NetworkDomainPermissionsToml::allowed_domains)
                .unwrap_or_default(),
            denied_domains: network
                .domains
                .as_ref()
                .and_then(codex_config::NetworkDomainPermissionsToml::denied_domains)
                .unwrap_or_default(),
        })
    }

    fn network_from_turn_context_item(
        turn_context_item: &TurnContextItem,
    ) -> Option<NetworkContext> {
        let TurnContextNetworkItem {
            allowed_domains,
            denied_domains,
        } = turn_context_item.network.as_ref()?;
        Some(NetworkContext {
            allowed_domains: allowed_domains.clone(),
            denied_domains: denied_domains.clone(),
        })
    }

    fn environments_from_turn_context(
        turn_context: &TurnContext,
    ) -> Option<Vec<EnvironmentContextEnvironment>> {
        if !turn_context.tools_config.multi_environment_tools {
            return None;
        }
        if turn_context.environments.is_empty() {
            return None;
        }

        Some(
            turn_context
                .environments
                .iter()
                .enumerate()
                .filter_map(|(index, environment)| {
                    Some(EnvironmentContextEnvironment {
                        id: environment.environment_id.clone()?,
                        cwd: environment.cwd.to_path_buf(),
                        primary: index == 0,
                    })
                })
                .collect(),
        )
    }

    fn environments_from_turn_context_item(
        turn_context_item: &TurnContextItem,
    ) -> Option<Vec<EnvironmentContextEnvironment>> {
        let environments = turn_context_item.environments.as_ref()?;
        if environments.is_empty() {
            return None;
        }

        Some(
            environments
                .iter()
                .enumerate()
                .map(|(index, environment)| EnvironmentContextEnvironment {
                    id: environment.environment_id.clone(),
                    cwd: environment.cwd.to_path_buf(),
                    primary: index == 0,
                })
                .collect(),
        )
    }

    fn environments_for_diff(
        before: &TurnContextItem,
        after: &TurnContext,
    ) -> Option<Vec<EnvironmentContextEnvironment>> {
        let before_environments = Self::environments_from_turn_context_item(before);
        let after_environments = Self::environments_from_turn_context(after);
        if before_environments == after_environments {
            None
        } else {
            after_environments
        }
    }
}

impl EnvironmentContext {
    /// Serializes the environment context to XML. Libraries like `quick-xml`
    /// require custom macros to handle Enums with newtypes, so we just do it
    /// manually, to keep things simple. Output looks like:
    ///
    /// ```xml
    /// <environment_context>
    ///   <cwd>...</cwd>
    ///   <shell>...</shell>
    /// </environment_context>
    /// ```
    pub fn serialize_to_xml(self) -> String {
        let mut lines = Vec::new();
        if let Some(cwd) = self.cwd {
            lines.push(format!("  <cwd>{}</cwd>", cwd.to_string_lossy()));
        }
        if let Some(environments) = self.environments {
            lines.push("  <environments>".to_string());
            for environment in environments {
                let primary = if environment.primary {
                    " primary=\"true\""
                } else {
                    ""
                };
                lines.push(format!(
                    "    <environment id=\"{}\"{}>",
                    environment.id, primary
                ));
                lines.push(format!(
                    "      <cwd>{}</cwd>",
                    environment.cwd.to_string_lossy()
                ));
                lines.push("    </environment>".to_string());
            }
            lines.push("  </environments>".to_string());
        }

        let shell_name = self.shell.name();
        lines.push(format!("  <shell>{shell_name}</shell>"));
        if let Some(current_date) = self.current_date {
            lines.push(format!("  <current_date>{current_date}</current_date>"));
        }
        if let Some(timezone) = self.timezone {
            lines.push(format!("  <timezone>{timezone}</timezone>"));
        }
        match self.network {
            Some(ref network) => {
                lines.push("  <network enabled=\"true\">".to_string());
                for allowed in &network.allowed_domains {
                    lines.push(format!("    <allowed>{allowed}</allowed>"));
                }
                for denied in &network.denied_domains {
                    lines.push(format!("    <denied>{denied}</denied>"));
                }
                lines.push("  </network>".to_string());
            }
            None => {
                // TODO(mbolin): Include this line if it helps the model.
                // lines.push("  <network enabled=\"false\" />".to_string());
            }
        }
        if let Some(subagents) = self.subagents {
            lines.push("  <subagents>".to_string());
            lines.extend(subagents.lines().map(|line| format!("    {line}")));
            lines.push("  </subagents>".to_string());
        }
        ENVIRONMENT_CONTEXT_FRAGMENT.wrap(lines.join("\n"))
    }
}

impl From<EnvironmentContext> for ResponseItem {
    fn from(ec: EnvironmentContext) -> Self {
        ENVIRONMENT_CONTEXT_FRAGMENT.into_message(ec.serialize_to_xml())
    }
}

#[cfg(test)]
#[path = "environment_context_tests.rs"]
mod tests;

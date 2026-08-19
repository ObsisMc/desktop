use crate::AgentRef;

/// Names one agent CLI Ora launches and supervises itself.
///
/// This is a launch descriptor, not an identity: it says which executable to run and with which
/// arguments. The identity a session is bound to is an [`AgentRef`], which every built-in CLI
/// supplies through [`AgentCli::agent_ref`] so that built-in and plugin-provided agents are
/// indistinguishable everywhere above the launch step.
///
/// The set shrinks as CLIs move out to plugins: OpenCode and Claude Code are no longer here
/// because `ora-space.opencode` and `ora-space.claude` are supplied by installed agent plugins,
/// which own their own executable lookup and launch arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentCli {
    Nga,
    CodeAgentCli,
    Codex,
}

impl AgentCli {
    pub const ALL: [Self; 3] = [Self::Nga, Self::CodeAgentCli, Self::Codex];

    /// Returns the namespaced identity this CLI provides, independent of enum declaration order.
    ///
    /// These are the same values persistence has always stored, so a built-in CLI keeps its
    /// identity when it is later repackaged as a plugin under the same id.
    pub fn agent_ref(self) -> AgentRef {
        let value = match self {
            Self::Nga => "ora-space.nga",
            Self::CodeAgentCli => "ora-space.codeagentcli",
            Self::Codex => "ora-space.codex",
        };
        // The literals above are non-empty, so parsing cannot fail; constructing through `parse`
        // keeps one validation path rather than a second way to build the value object.
        AgentRef::parse(value).unwrap_or_else(|_| unreachable!("built-in agent ids are valid"))
    }

    /// Returns the executable basename used by the cross-platform PATH lookup.
    pub fn executable_name(self) -> &'static str {
        match self {
            Self::Nga => "nga",
            Self::CodeAgentCli => "codeagentcli",
            Self::Codex => "codex-acp",
        }
    }

    /// Returns the child process arguments used to start ACP over stdio.
    ///
    /// Ora's own CLIs (Nga, CodeAgentCli) expose ACP behind an `acp`
    /// subcommand. Codex is instead fronted by a dedicated `codex-acp` adapter
    /// binary, which speaks ACP directly with no subcommand.
    pub fn launch_arguments(self) -> &'static [&'static str] {
        match self {
            Self::Nga | Self::CodeAgentCli => &["acp"],
            Self::Codex => &[],
        }
    }
}

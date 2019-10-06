pub mod aws;
pub mod battery;
pub mod character;
pub mod conda;
pub mod dotnet;
pub mod kubernetes;
pub mod rust;
pub mod time;

use crate::config::{ModuleConfig, RootModuleConfig};

use starship_module_config_derive::ModuleConfig;

#[derive(Clone, ModuleConfig)]
pub struct StarshipRootConfig<'a> {
    pub add_newline: bool,
    pub prompt_order: Vec<&'a str>,
}

impl<'a> RootModuleConfig<'a> for StarshipRootConfig<'a> {
    fn new() -> Self {
        StarshipRootConfig {
            add_newline: true,
            // List of default prompt order
            // NOTE: If this const value is changed then Default prompt order subheading inside
            // prompt heading of config docs needs to be updated according to changes made here.
            prompt_order: vec![
                "username",
                "hostname",
                "kubernetes",
                "directory",
                "git_branch",
                "git_state",
                "git_status",
                "package",
                // ↓ Toolchain version modules ↓
                // (Let's keep these sorted alphabetically)
                "dotnet",
                "golang",
                "java",
                "nodejs",
                "python",
                "ruby",
                "rust",
                // ↑ Toolchain version modules ↑
                "nix_shell",
                "conda",
                "memory_usage",
                "aws",
                "env_var",
                "cmd_duration",
                "line_break",
                "jobs",
                #[cfg(feature = "battery")]
                "battery",
                "time",
                "character",
            ],
        }
    }
}

/// Get the default disabled option in a module by name
pub fn is_module_disabled_by_default(name: &str) -> bool {
    match name {
        "aws" => aws::AwsConfig::new().disabled,
        "battery" => battery::BatteryConfig::new().disabled,
        "character" => character::CharacterConfig::new().disabled,
        "conda" => conda::CondaConfig::new().disabled,
        "dotnet" => dotnet::DotnetConfig::new().disabled,
        "kubernetes" => kubernetes::KubernetesConfig::new().disabled,
        "rust" => rust::RustConfig::new().disabled,
        "time" => time::TimeConfig::new().disabled,
        _ => true,
    }
}

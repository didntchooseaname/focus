mod catalog;
mod export;
mod exposure;
mod ext;
mod fault;
mod graph;
mod hidden_links;
pub mod ipc;
pub mod ops;
mod paths;
mod rules;
pub mod server;
mod specs;
mod tools;

pub use catalog::{McpHost, PromptArgument, PromptInfo, Resource};
pub use tools::{Confirm, VaultTools, WriteProposal};

#[cfg(test)]
mod tests_ext;
#[cfg(test)]
mod tests_rules;
#[cfg(test)]
mod testvault;

use std::time::Duration;

use crate::agent::{self, ToolHost, ToolOutcome};
use crate::{Cancel, ToolCall, ToolSpec};

use super::{McpError, Server, ServerConfig};

#[derive(Debug)]
pub struct ConfirmRequest<'a> {
    pub server: &'a str,
    pub tool: &'a str,
    pub arguments: &'a str,
}

type Confirm = Box<dyn Fn(&ConfirmRequest, &Cancel) -> bool + Send + Sync>;

pub struct CompositeHost<H: ToolHost> {
    internal: H,
    servers: Vec<Server>,
    confirm: Confirm,
    call_timeout: Duration,
}

impl<H: ToolHost> CompositeHost<H> {
    pub fn new(
        internal: H,
        servers: Vec<Server>,
        confirm: impl Fn(&ConfirmRequest, &Cancel) -> bool + Send + Sync + 'static,
    ) -> Self {
        CompositeHost {
            internal,
            servers,
            confirm: Box::new(confirm),
            call_timeout: Duration::from_secs(60),
        }
    }

    pub fn with_call_timeout(mut self, timeout: Duration) -> Self {
        self.call_timeout = timeout;
        self
    }

    pub fn servers(&self) -> &[Server] {
        &self.servers
    }

    pub fn requires_confirmation(&self, tool_name: &str) -> bool {
        match self.servers.iter().find(|s| s.owns(tool_name)) {
            Some(server) => server.requires_confirmation(tool_name),
            None => {
                agent::is_write_tool(tool_name)
                    || !self.internal.specs().iter().any(|s| s.name == tool_name)
            }
        }
    }

    pub fn start_servers(configs: &[ServerConfig]) -> (Vec<Server>, Vec<(String, McpError)>) {
        let mut started = Vec::new();
        let mut failed = Vec::new();
        for config in configs.iter().filter(|c| c.enabled) {
            match Server::start(config) {
                Ok(server) => started.push(server),
                Err(error) => failed.push((config.name.clone(), error)),
            }
        }
        (started, failed)
    }
}

impl<H: ToolHost> ToolHost for CompositeHost<H> {
    fn specs(&self) -> Vec<ToolSpec> {
        let mut specs = self.internal.specs();
        for server in &self.servers {
            specs.extend(server.tools());
        }
        specs
    }

    fn call(&self, call: &ToolCall, cancel: &Cancel) -> ToolOutcome {
        let Some(server) = self.servers.iter().find(|s| s.owns(&call.name)) else {
            return self.internal.call(call, cancel);
        };
        if server.requires_confirmation(&call.name) {
            let request = ConfirmRequest {
                server: server.name(),
                tool: server.raw_name(&call.name).unwrap_or(&call.name),
                arguments: &call.arguments,
            };
            if !(self.confirm)(&request, cancel) {
                return ToolOutcome::Denied;
            }
        }
        server.call(&call.name, &call.arguments, cancel, self.call_timeout)
    }
}

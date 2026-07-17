use std::io::{BufRead, Write};

use super::{Deps, Interface};

pub struct LocalInterface;

impl Interface for LocalInterface {
    fn name(&self) -> &str {
        "local"
    }

    fn run(&self, mut deps: Deps) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let sess = deps.store.create()?;
        let mut asession = deps.new_session(sess);

        println!("{} local mode — ctrl+d to exit", deps.agent_name);
        println!();

        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();

        let rt = tokio::runtime::Runtime::new()?;

        loop {
            print!("> ");
            stdout.flush()?;

            let mut line = String::new();
            let n = stdin.lock().read_line(&mut line)?;
            if n == 0 {
                println!();
                return Ok(());
            }

            let text = line.trim().to_string();
            if text.is_empty() {
                continue;
            }

            let mut events = asession.prompt(&text);

            loop {
                let ev = rt.block_on(events.recv());
                match ev {
                    Some(ev) => match crate::agent::print_event(&ev, true) {
                        crate::agent::StdioEvent::Handled => {}
                        crate::agent::StdioEvent::ToolError { name, error } => {
                            println!("[{} error: {}]", name, error);
                        }
                        crate::agent::StdioEvent::Error(msg) => {
                            println!("\nError: {}", msg);
                        }
                        crate::agent::StdioEvent::AgentDone => {}
                    },
                    None => break,
                }
            }
            println!();
            println!();
        }
    }
}

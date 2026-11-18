use std::io::{BufRead, Write};

use focus_ai::{
    ApiKey, Cancel, ChatRequest, Client, Config, Message, Role, StreamEvent, Usage,
    estimate_cost_usd, prompts,
};

fn main() -> anyhow::Result<()> {
    let Some(key) = std::env::var("DEEPSEEK_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
    else {
        eprintln!("DEEPSEEK_API_KEY is not set: nothing is sent.");
        std::process::exit(2);
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut config = Config::new(ApiKey::new(key));
    if let Some(position) = args.iter().position(|a| a == "--model") {
        config.model = args.get(position + 1).cloned().unwrap_or(config.model);
    }
    config.thinking = !args.iter().any(|a| a == "--no-thinking");
    let client = Client::new(config);

    if args.iter().any(|a| a == "--balance") {
        let balance = client.balance()?;
        println!("available: {}", balance.available);
        for (currency, total) in balance.balances {
            println!("{currency} {total}");
        }
        return Ok(());
    }

    let mut messages = vec![Message {
        role: Role::System,
        content: prompts::system(false, "", "en"),
        reasoning: None,
        tool_calls: vec![],
        tool_call_id: None,
    }];
    let mut total = Usage::default();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        messages.push(Message {
            role: Role::User,
            content: line,
            reasoning: None,
            tool_calls: vec![],
            tool_call_id: None,
        });
        let request = ChatRequest {
            messages: messages.clone(),
            ..ChatRequest::default()
        };
        let mut usage = Usage::default();
        let reply = client.chat_stream(&request, &Cancel::new(), &mut |event| match event {
            StreamEvent::Reasoning(text) => eprint!("{text}"),
            StreamEvent::Content(text) => {
                print!("{text}");
                let _ = std::io::stdout().flush();
            }
            StreamEvent::Usage(u) => usage = u,
            _ => {}
        })?;
        println!();
        let cost = estimate_cost_usd(&client.config().model, &usage)
            .map_or("unknown".to_string(), |c| format!("{c:.6} USD"));
        eprintln!(
            "[{} input of which {} cached, {} output, estimated cost {cost}]",
            usage.prompt_tokens, usage.cache_hit_tokens, usage.completion_tokens
        );
        total.prompt_tokens += usage.prompt_tokens;
        total.completion_tokens += usage.completion_tokens;
        messages.push(reply);
    }
    eprintln!(
        "[total: {} input, {} output]",
        total.prompt_tokens, total.completion_tokens
    );
    Ok(())
}

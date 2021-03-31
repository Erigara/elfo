use elfo::prelude::*;
use serde::Deserialize;

#[message]
struct Increment;

#[message]
struct Added(u32);

#[message(ret = u32)]
struct Summarize;

#[derive(Debug, Deserialize)]
struct Config {
    step: u32,
}

async fn summator(mut ctx: Context<Config>) {
    let mut sum = 0;

    while let Some(envelope) = ctx.recv().await {
        msg!(match envelope {
            Increment => {
                let step = ctx.config().step;
                sum += step;
                let _ = ctx.send(Added(step)).await;
            }
            (Summarize, token) => {
                ctx.respond(token, sum);
            }
            _ => {}
        })
    }
}

pub fn summators() -> Schema {
    ActorGroup::new().config::<Config>().exec(summator)
}

#[tokio::test]
async fn it_works() {
    // Define a config (usually using `toml!` or `json!`).
    let config = toml::toml! {
        step = 20
    };

    // Wrap the actor group to take control over it.
    let mut proxy = elfo::test::proxy(summators(), config).await;

    // How to send messages to the group.
    proxy.send(Increment).await;
    proxy.send(Increment).await;

    // How to check actors' output.
    assert_msg!(proxy.recv().await, Added(15u32..=35)); // Note: rhs is a pattern.

    // FIXME: assert_msg_eq!(proxy.recv(), Added(20));

    // How to check request-response.
    assert_eq!(proxy.request(Summarize).await, 40);

    // TODO: check that there are no more messages.
}

fn main() {
    panic!("run `cargo test`");
}

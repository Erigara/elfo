use std::sync::Arc;

use tokio::task::JoinHandle;
use tracing::{error, info};

use elfo_core as elfo;
use elfo_macros::{message, msg_raw as msg};

use elfo::{
    messages::ConfigUpdated, scope, time::Interval, trace_id, ActorGroup, Context, Local, Schema,
};

use crate::{config::Config, render::Renderer, storage::Storage};

struct Telemeter {
    ctx: Context<Config>,
    storage: Arc<Storage>,
    renderer: Renderer,
}

#[message(ret = String, elfo = elfo_core)]
struct Render;

#[message(elfo = elfo_core)]
struct CompactionTick;

#[message(elfo = elfo_core)]
struct ServerFailed(Arc<Local<hyper::Error>>);

pub(crate) fn new(storage: Arc<Storage>) -> Schema {
    ActorGroup::new()
        .config::<Config>()
        .exec(move |ctx| Telemeter::new(ctx, storage.clone()).main())
}

impl Telemeter {
    pub(crate) fn new(ctx: Context<Config>, storage: Arc<Storage>) -> Self {
        let mut renderer = Renderer::default();
        renderer.configure(ctx.config());

        Self {
            ctx,
            storage,
            renderer,
        }
    }

    async fn main(mut self) {
        let interval = Interval::new(|| CompactionTick);
        let mut ctx = self.ctx.clone().with(&interval);

        let mut address = ctx.config().address;
        let mut server = start_server(&self.ctx);

        interval.set_period(ctx.config().compaction_interval);

        while let Some(envelope) = ctx.recv().await {
            msg!(match envelope {
                ConfigUpdated => {
                    let config = ctx.config();

                    if config.address != address {
                        info!("address changed, rerun the server");
                        server.abort();
                        address = config.address;
                        server = start_server(&self.ctx);
                    }

                    self.renderer.configure(config);
                }
                (Render, token) => {
                    // Rendering includes compaction, skip extra compaction tick.
                    interval.reset();

                    let snapshot = self.storage.snapshot();
                    let descriptions = self.storage.descriptions();
                    let output = self.renderer.render(snapshot, &descriptions);
                    ctx.respond(token, output);
                }
                CompactionTick => {
                    self.storage.compact();
                }
                ServerFailed(error) => {
                    error!(error = %&**error, "server failed");
                    panic!("server failed");
                }
            });
        }
    }
}

fn start_server(ctx: &Context<Config>) -> JoinHandle<()> {
    use hyper::{
        server::{conn::AddrStream, Server},
        service::{make_service_fn, service_fn},
        Body, Error as HyperError, Response,
    };

    let address = ctx.config().address;
    let ctx = Arc::new(ctx.pruned());
    let ctx1 = ctx.clone();

    let scope = scope::expose();
    let scope1 = scope.clone();

    let serving = async move {
        let server = Server::try_bind(&address)?;
        let make_svc = make_service_fn(move |_socket: &AddrStream| {
            let ctx = ctx.clone();
            let scope = scope.clone();

            async move {
                Ok::<_, HyperError>(service_fn(move |_| {
                    let ctx = ctx.clone();
                    let scope = scope.clone();

                    let f = async move {
                        let output = ctx
                            .request(Render)
                            .from(ctx.addr())
                            .resolve()
                            .await
                            .expect("failed to send to the telemeter");
                        Ok::<_, HyperError>(Response::new(Body::from(output)))
                    };

                    scope.set_trace_id(trace_id::generate());
                    scope.within(f)
                }))
            }
        });
        server.serve(make_svc).await
    };

    tokio::spawn(async move {
        if let Err(err) = serving.await {
            let f = async {
                let _ = ctx1.send(ServerFailed(Arc::new(Local::from(err)))).await;
            };

            scope1.set_trace_id(trace_id::generate());
            scope1.within(f).await;
        }
    })
}
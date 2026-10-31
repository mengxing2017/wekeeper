//! HTTP/WebSocket transport only. No SQL or business execution in socket frames.
use crate::{
    bootstrap::Application,
    business::{Actor, Error, Result},
    distribution::domain::device_control::*,
    http::jsonapi::{Input, resource},
};
use axum::{
    Router,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use uuid::Uuid;
fn token(h: &HeaderMap) -> Result<&str> {
    h.get("authorization")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(Error::Unauthorized)
}
async fn credential(a: &Application, h: &HeaderMap) -> Result<Credential> {
    a.control_credential(token(h)?).await
}
async fn open(State(a): State<Application>, h: HeaderMap) -> Result<Response> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "device-control-frames",
        a.device_control()
            .device(
                &c,
                Operation::Open {
                    transport: "https".into(),
                },
            )
            .await?,
    )
    .into_response())
}
async fn pulse(State(a): State<Application>, h: HeaderMap, input: Input) -> Result<Response> {
    let c = credential(&a, &h).await?;
    let p = input.decode::<Pulse>("device-control-pulses")?;
    Ok(resource(
        "device-control-frames",
        a.device_control().device(&c, Operation::Pulse(p)).await?,
    )
    .into_response())
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Renewal {
    candidate_hash: String,
}
async fn renew(State(a): State<Application>, h: HeaderMap, input: Input) -> Result<Response> {
    let c = credential(&a, &h).await?;
    let r = input.decode::<Renewal>("device-credential-renewals")?;
    Ok(resource(
        "device-control-frames",
        a.device_control()
            .device(
                &c,
                Operation::Renew {
                    candidate_hash: r.candidate_hash,
                },
            )
            .await?,
    )
    .into_response())
}
async fn activate(State(a): State<Application>, h: HeaderMap) -> Result<Response> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "device-control-frames",
        a.device_control().device(&c, Operation::Activate).await?,
    )
    .into_response())
}
async fn presence(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
) -> Result<Response> {
    Ok(resource(
        "device-control-presence",
        a.device_control().presence(actor.id, id).await?,
    )
    .into_response())
}
async fn enqueue(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<Response> {
    let key = h
        .get("idempotency-key")
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");
    Ok(resource(
        "device-control-commands",
        a.device_control()
            .enqueue(
                actor.id,
                id,
                key,
                input.decode::<CommandInput>("device-control-commands")?,
            )
            .await?,
    )
    .into_response())
}
async fn channel(
    State(a): State<Application>,
    h: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response> {
    let c = credential(&a, &h).await?;
    Ok(ws
        .max_message_size(65536)
        .max_frame_size(65536)
        .on_upgrade(move |socket| run(a, c, socket)))
}
async fn run(a: Application, c: Credential, mut socket: WebSocket) {
    let service = a.device_control();
    let Ok(frame) = service
        .device(
            &c,
            Operation::Open {
                transport: "wss".into(),
            },
        )
        .await
    else {
        return;
    };
    let id = frame.connection_id;
    let epoch = frame.epoch;
    if !matches!(
        tokio::time::timeout(std::time::Duration::from_secs(5), send(&mut socket, &frame)).await,
        Ok(Ok(()))
    ) {
        return;
    }
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
    let mut last = String::new();
    let mut budget_started = tokio::time::Instant::now();
    let mut messages = 0u32;
    loop {
        let (result, reply) = tokio::select! {
         _=tick.tick()=>(service.device(&c,Operation::Poll{connection_id:id,epoch}).await,false),
         incoming=socket.recv()=>{
          match incoming {
           Some(Ok(Message::Text(text)))=>{
            if budget_started.elapsed().as_secs() >= 60 {budget_started=tokio::time::Instant::now();messages=0;}
            messages+=1;
            if messages>120 {break;}
            match serde_json::from_str::<Pulse>(&text){
            Ok(p) if p.connection_id==id&&p.epoch==epoch=>(service.device(&c,Operation::Pulse(p)).await,true),
            _=>break,
           }},
           Some(Ok(Message::Ping(_)))|Some(Ok(Message::Pong(_)))=>continue,
           _=>break,
          }
         }
        };
        match result {
            Ok(f) => {
                let signature = format!(
                    "{}:{}",
                    f.desired_revision,
                    serde_json::to_string(&f.commands).unwrap_or_default()
                );
                if reply || signature != last {
                    last = signature;
                    if !matches!(
                        tokio::time::timeout(
                            std::time::Duration::from_secs(5),
                            send(&mut socket, &f)
                        )
                        .await,
                        Ok(Ok(()))
                    ) {
                        break;
                    }
                }
            }
            Err(e) => {
                let _=tokio::time::timeout(std::time::Duration::from_secs(5),socket.send(Message::Text(serde_json::json!({"error":match e {Error::Unauthorized=>"unauthorized",_=>"control_session_changed"}}).to_string().into()))).await;
                break;
            }
        }
    }
    let _ = service
        .device(
            &c,
            Operation::Close {
                connection_id: id,
                epoch,
            },
        )
        .await;
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        socket.send(Message::Close(None)),
    )
    .await;
}
async fn send(socket: &mut WebSocket, frame: &Frame) -> std::result::Result<(), axum::Error> {
    socket
        .send(Message::Text(
            serde_json::to_string(frame).unwrap_or_default().into(),
        ))
        .await
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v1/device-control/channel", get(channel))
        .route("/api/v1/device-control/session", post(open))
        .route("/api/v1/device-control/pulses", post(pulse))
        .route("/api/v1/device-control/credential-renewals", post(renew))
        .route(
            "/api/v1/device-control/credential-activations",
            post(activate),
        )
        .route(
            "/api/v1/platform/execution-devices/{id}/control",
            get(presence).post(enqueue),
        )
}

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
    if p.probe_id.is_some() {
        return Err(Error::Invalid("主动探测响应必须来自原控制连接".into()));
    }
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
        let _ = service
            .device(
                &c,
                Operation::Close {
                    connection_id: id,
                    epoch,
                },
            )
            .await;
        return;
    }
    // Command latency remains bounded by five seconds; this is not a heartbeat.
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut probes = ProbeSchedule::new(tokio::time::Instant::now());
    let mut heartbeat_interval = frame.heartbeat_interval_seconds;
    let mut last = String::new();
    let mut budget_started = tokio::time::Instant::now();
    let mut messages = 0u32;
    loop {
        let (result, reply, probe_response) = tokio::select! {
         _=tokio::time::sleep_until(probes.next_due(heartbeat_interval))=>(service.device(&c,Operation::Poll{connection_id:id,epoch}).await,false,false),
         _=tick.tick()=>(service.device(&c,Operation::Poll{connection_id:id,epoch}).await,false,false),
         incoming=socket.recv()=>{
          match incoming {
           Some(Ok(Message::Text(text)))=>{
            if budget_started.elapsed().as_secs() >= 60 {budget_started=tokio::time::Instant::now();messages=0;}
            messages+=1;
            if messages>120 {break;}
            match serde_json::from_str::<Pulse>(&text){
            Ok(p) if p.connection_id==id&&p.epoch==epoch=>{
                if let Some(probe) = p.probe_id
                    && !probes.accept(probe, tokio::time::Instant::now()) { continue; }
                let probe_response = p.probe_id.is_some();
                (service.device(&c,Operation::Pulse(p)).await,true,probe_response)
            },
            _=>break,
           }},
           Some(Ok(Message::Ping(_)))|Some(Ok(Message::Pong(_)))=>continue,
           _=>break,
          }
         }
        };
        match result {
            Ok(mut f) => {
                heartbeat_interval = f.heartbeat_interval_seconds;
                let now = tokio::time::Instant::now();
                if reply && f.heartbeat_fresh && probe_response {
                    probes.activity(now);
                }
                if !(reply && f.heartbeat_fresh) {
                    f.probe_id = probes.due(now, f.heartbeat_interval_seconds);
                }
                let signature = format!(
                    "{}:{}",
                    f.desired_revision,
                    serde_json::to_string(&f.commands).unwrap_or_default()
                );
                if reply || f.probe_id.is_some() || signature != last {
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
                    if reply && f.heartbeat_fresh && !probe_response {
                        if let Some(heartbeat_id) = f.heartbeat_id
                            && service
                                .device(
                                    &c,
                                    Operation::ConfirmHeartbeat {
                                        connection_id: id,
                                        epoch,
                                        heartbeat_id,
                                    },
                                )
                                .await
                                .is_err()
                        {
                            break;
                        }
                        probes.activity(tokio::time::Instant::now());
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

// A successful device pulse suppresses redundant server probes. A probe timeout
// does not close the socket: persisted presence owns the full three-cycle window.
struct ProbeSchedule {
    last_activity: tokio::time::Instant,
    pending: Option<(Uuid, tokio::time::Instant)>,
}
impl ProbeSchedule {
    fn new(now: tokio::time::Instant) -> Self {
        Self {
            last_activity: now,
            pending: None,
        }
    }
    fn activity(&mut self, now: tokio::time::Instant) {
        self.last_activity = now;
        self.pending = None;
    }
    fn next_due(&self, interval: u16) -> tokio::time::Instant {
        self.last_activity + std::time::Duration::from_secs(u64::from(interval))
    }
    fn due(&mut self, now: tokio::time::Instant, interval: u16) -> Option<Uuid> {
        if now.duration_since(self.last_activity).as_secs() < u64::from(interval) {
            return None;
        }
        let id = Uuid::new_v4();
        self.last_activity = now;
        self.pending = Some((id, now));
        Some(id)
    }
    fn accept(&mut self, id: Uuid, now: tokio::time::Instant) -> bool {
        if self.pending.is_some_and(|(expected, sent)| {
            expected == id && now.duration_since(sent).as_secs() < 10
        }) {
            self.pending = None;
            true
        } else {
            false
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn probes_share_activity_window_and_require_one_timely_correlated_reply() {
        let start = tokio::time::Instant::now();
        let mut schedule = ProbeSchedule::new(start);
        assert!(schedule.due(start + Duration::from_secs(59), 60).is_none());
        let probe = schedule.due(start + Duration::from_secs(60), 60).unwrap();
        assert!(!schedule.accept(Uuid::new_v4(), start + Duration::from_secs(61)));
        assert!(schedule.accept(probe, start + Duration::from_secs(61)));
        assert!(!schedule.accept(probe, start + Duration::from_secs(62)));
        schedule.activity(start + Duration::from_secs(61));
        assert_eq!(schedule.next_due(60), start + Duration::from_secs(121));
        assert!(schedule.due(start + Duration::from_secs(120), 60).is_none());
        let expired = schedule.due(start + Duration::from_secs(121), 60).unwrap();
        assert!(!schedule.accept(expired, start + Duration::from_secs(131)));
    }
}

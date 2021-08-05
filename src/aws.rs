use bytes::Bytes;
use log::{debug, error, warn};
use rusoto_core::Region;
use rusoto_lambda::{InvocationRequest, Lambda, LambdaClient};
use serde::{Deserialize, Serialize};
use std::io;
use std::time::Duration;
use tiltak::position::Move;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum TimeControl {
    FixedNodes(u64),
    Time(Duration, Duration), // Total time left, increment
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Event {
    pub size: usize,
    pub moves: Vec<Move>,
    pub time_control: TimeControl,
    pub dirichlet_noise: Option<f32>,
    pub rollout_depth: u16,
    pub rollout_temperature: f64,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Output {
    pub pv: Vec<Move>,
    pub score: f32,
}

pub async fn pv_aws(
    aws_function_name: &str,
    size: usize,
    moves: Vec<Move>,
    nodes: u64,
) -> io::Result<Output> {
    let event = Event {
        size,
        moves,
        time_control: TimeControl::FixedNodes(nodes),
        dirichlet_noise: None,
        rollout_depth: 0,
        rollout_temperature: 0.2,
    };
    let client = LambdaClient::new(Region::UsEast2);

    let request = InvocationRequest {
        client_context: None,
        function_name: aws_function_name.to_string(),
        invocation_type: Some("RequestResponse".to_string()),
        log_type: None,
        payload: Some(Bytes::copy_from_slice(&serde_json::to_vec(&event).unwrap())),
        qualifier: None,
    };

    let result = client.invoke(request).await;
    match result {
        Ok(response) => {
            if let Some(status_code) = response.status_code {
                if status_code / 100 == 2 {
                    debug!("Got HTTP response {} from aws", status_code);
                } else {
                    error!("Got HTTP response {} from aws", status_code);
                }
            } else {
                warn!("AWS response contained no status code");
            }
            if let Some(payload) = response.payload {
                Ok(serde_json::from_str(
                    std::str::from_utf8(&payload)
                        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?,
                )?)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "AWS response contained no payload",
                ))
            }
        }
        Err(err) => Err(io::Error::new(io::ErrorKind::Other, err)),
    }
}

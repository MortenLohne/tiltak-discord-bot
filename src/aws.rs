use crate::AWS_FUNCTION_NAME;
use bytes::Bytes;
use log::{debug, error, warn};
use rusoto_core::Region;
use rusoto_lambda::{InvocationRequest, Lambda, LambdaClient};
use serde::{Deserialize, Serialize};
use std::io;
use std::time::Duration;
use tiltak::position::Komi;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum TimeControl {
    FixedNodes(u64),
    Time(Duration, Duration), // Total time left, increment
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Event {
    pub size: usize,
    pub tps: Option<String>,
    pub moves: Vec<String>,
    pub time_control: TimeControl,
    pub komi: f64, // "Main" komi setting, used to determine the game result at terminal nodes
    pub eval_komi: Option<f64>, // Komi used for heuristic evaluation. Default to the main komi, but not all komis are supported
    pub dirichlet_noise: Option<f32>,
    pub rollout_depth: u16,
    pub rollout_temperature: f64,
}

#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct Output {
    pub pv: Vec<String>,
    pub score: f32,
    pub nodes: u64,
    pub mem_usage: u64,
    pub time_taken: Duration,
}

pub async fn pv_aws(
    size: usize,
    tps: Option<String>,
    moves: Vec<String>,
    nodes: u64,
    komi: Komi,
    eval_komi: Komi,
) -> io::Result<Output> {
    let is_white = moves.len() % 2 != 1;
    let event = Event {
        size,
        tps,
        moves,
        time_control: TimeControl::FixedNodes(nodes),
        komi: komi.into(),
        eval_komi: Some(eval_komi.into()),
        dirichlet_noise: None,
        rollout_depth: 0,
        rollout_temperature: 0.2,
    };
    let client = LambdaClient::new(Region::UsEast2);

    let request = InvocationRequest {
        client_context: None,
        function_name: AWS_FUNCTION_NAME.get().unwrap().clone(),
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
                let payload_string = std::str::from_utf8(&payload)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
                debug!("AWS event: {:?}", event);
                debug!("AWS payload: {}", payload_string);
                let mut output: Output = serde_json::from_str(payload_string)?;
                // Always show score from white's perspective
                if is_white {
                    output.score = 1.0 - output.score;
                }
                Ok(output)
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

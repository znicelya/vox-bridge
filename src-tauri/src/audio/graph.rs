use crate::{
    audio::platform::{AudioDirection, AudioEndpoint},
    error::{AppError, AppResult},
};

use super::router::{Direction, RouteTarget};

#[derive(Clone, Debug)]
pub struct AudioGraph {
    routes: Vec<Route>,
    endpoints: Vec<AudioEndpoint>,
}

#[derive(Clone, Debug)]
struct Route {
    direction: Direction,
    source: AudioEndpoint,
    destination: AudioEndpoint,
    target: RouteTarget,
}

pub type LiveGraph = AudioGraph;
pub type CallGraph = AudioGraph;

impl AudioGraph {
    pub fn for_live(input: AudioEndpoint, m1: AudioEndpoint) -> AppResult<LiveGraph> {
        validate_endpoint(&input, AudioDirection::Input, "live input")?;
        validate_endpoint(&m1, AudioDirection::Output, "M1 output")?;
        validate_distinct(&[&input, &m1])?;
        Ok(Self {
            routes: vec![Route {
                direction: Direction::Live,
                source: input.clone(),
                destination: m1.clone(),
                target: RouteTarget::M1,
            }],
            endpoints: vec![input, m1],
        })
    }

    pub fn for_call(
        a_input: AudioEndpoint,
        m2_input: AudioEndpoint,
        m1_output: AudioEndpoint,
        physical_output: AudioEndpoint,
    ) -> AppResult<CallGraph> {
        validate_endpoint(&a_input, AudioDirection::Input, "A input")?;
        validate_endpoint(&m2_input, AudioDirection::Input, "M2 input")?;
        validate_endpoint(&m1_output, AudioDirection::Output, "M1 output")?;
        validate_endpoint(&physical_output, AudioDirection::Output, "physical output")?;
        validate_distinct(&[&a_input, &m2_input, &m1_output, &physical_output])?;
        Ok(Self {
            routes: vec![
                Route {
                    direction: Direction::AtoB,
                    source: a_input.clone(),
                    destination: m1_output.clone(),
                    target: RouteTarget::M1,
                },
                Route {
                    direction: Direction::BtoA,
                    source: m2_input.clone(),
                    destination: physical_output.clone(),
                    target: RouteTarget::PhysicalOutput,
                },
            ],
            endpoints: vec![a_input, m2_input, m1_output, physical_output],
        })
    }

    pub fn directions(&self) -> Vec<Direction> {
        self.routes.iter().map(|route| route.direction).collect()
    }
    pub fn target(&self, direction: Direction) -> Option<RouteTarget> {
        self.routes
            .iter()
            .find(|route| route.direction == direction)
            .map(|route| route.target)
    }

    pub(crate) fn route(&self, direction: Direction) -> Option<&Route> {
        self.routes
            .iter()
            .find(|route| route.direction == direction)
    }
    pub(crate) fn route_for_target(&self, target: RouteTarget) -> Option<&Route> {
        self.routes.iter().find(|route| route.target == target)
    }
    pub(crate) fn first_route(&self) -> Option<&Route> {
        self.routes.first()
    }
    pub(crate) fn endpoints(&self) -> &[AudioEndpoint] {
        &self.endpoints
    }
}

impl Route {
    pub(crate) fn direction(&self) -> Direction {
        self.direction
    }

    pub(crate) fn source(&self) -> &AudioEndpoint {
        &self.source
    }
    pub(crate) fn destination(&self) -> &AudioEndpoint {
        &self.destination
    }
}

fn validate_endpoint(
    endpoint: &AudioEndpoint,
    direction: AudioDirection,
    role: &str,
) -> AppResult<()> {
    if endpoint.id.trim().is_empty() || endpoint.display_name.trim().is_empty() {
        return Err(AppError::InvalidRequest(format!(
            "{role} endpoint must have an ID and display name"
        )));
    }
    if endpoint.direction != direction {
        return Err(AppError::InvalidRequest(format!(
            "{role} endpoint {} has the wrong direction",
            endpoint.id
        )));
    }
    if !endpoint.ready {
        return Err(AppError::InvalidRequest(format!(
            "{role} endpoint {} is not ready",
            endpoint.id
        )));
    }
    Ok(())
}

fn validate_distinct(endpoints: &[&AudioEndpoint]) -> AppResult<()> {
    for (index, endpoint) in endpoints.iter().enumerate() {
        if endpoints[..index]
            .iter()
            .any(|other| other.id == endpoint.id)
        {
            return Err(AppError::InvalidRequest(format!(
                "audio graph endpoint ID {} is used more than once",
                endpoint.id
            )));
        }
    }
    Ok(())
}

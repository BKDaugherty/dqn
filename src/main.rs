use anyhow::{Context, Result};
use env_logger::Env;
use gym_rs::{CartPoleEnv, GifRender, GymEnv};
use log::info;
use std::time::SystemTime;
use structopt::StructOpt;
use tch::nn;

pub mod agent;
pub mod dqn;
pub mod environment;

use crate::agent::{Agent, Stage};
use crate::dqn::{DQNAgent, Sequence, SequenceNode};
use crate::environment::{Action, EpisodeResult, GameState, GameStatus, Reward};

#[derive(Debug, StructOpt)]
struct Args {
    /// Number of training episodes to run
    #[structopt(long, default_value = "20")]
    episodes: u32,
}

impl Agent for DQNAgent {
    fn choose_action(&mut self, state: GameState) -> Action {
        self.model.choose_action(state, Stage::Test)
    }
}

fn run_episode_train_dqn(dqn: &mut DQNAgent, env: &mut dyn GymEnv) -> Result<EpisodeResult> {
    let mut episode_reward = 0.0;
    let mut game_state = GameState::new(env.reset(), false);
    let mut sequence = Sequence {
        initial_state: game_state.clone(),
        nodes: vec![],
    };

    loop {
        // Run a forward pass to get the best action
        let action = dqn.model.choose_action(game_state.clone(), Stage::Train);

        // emulate that action in the environment
        let (next_state, reward, game_complete, info) = env.step(action.clone().0);
        let next_state = GameState::new(next_state, game_complete);

        // Append the result to the sequence
        sequence.nodes.push(SequenceNode {
            state: next_state.clone(),
            action: action.clone(),
        });

	// Store this transition into the replay memory
        dqn.replay_memory
            .store(sequence.clone(), action.clone(), Reward(reward), next_state)
            .context("storing memory node")?;

	// Update gradients based on a bunch of samples, not just this one to avoid
	// locality problems
        let minibatch = dqn.replay_memory.sample();
        for transition in minibatch {
            // Perform gradient descent update
        }

        if let Some(info) = info {
            info!("Step information: {}", info);
        }

        if game_complete {
            break;
        }
    }
    todo!()
}

fn run_episode(
    agent: &mut dyn Agent,
    env: &mut dyn GymEnv,
    render: &mut Option<GifRender>,
) -> Result<EpisodeResult> {
    let mut total_reward = 0.0;
    // Reset the environment before running through an episode
    let mut game_state = GameState::new(env.reset(), false);

    // Run the agent on the environment
    loop {
        let action = agent.choose_action(game_state);

        // Run a step of the environment
        let (next_state, reward, game_complete, info) = env.step(action.0);

        total_reward += reward;

        if let Some(info) = info {
            info!("Step information: {}", info);
        }
        // Advance the game state
        let next_state = GameState::new(next_state, game_complete);
        game_state = next_state;

        // Render the frame
        if let Some(render) = render.as_mut() {
            env.render(render);
        }

        // Break if the game is complete
        if game_complete {
            break;
        }
    }

    Ok(EpisodeResult { total_reward })
}

fn main() -> Result<()> {
    // Create our logger
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let args = Args::from_args();

    // Create a variable store that locates its variables on the systems CPU
    let vs = nn::VarStore::new(tch::Device::Cpu);
    let mut agent = DQNAgent::new(&vs.root());

    // Train the model
    for episode in 1..args.episodes {
        // Create the cart pole environment
        let mut env = CartPoleEnv::default();

        // Get a timestamp to mark our results
        let episode_begin = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();

        let episode_result = run_episode_train_dqn(&mut agent, &mut env)
            .context(format!("Running episode {}", episode))?;

        info!(
            "Episode {} finished successfully, total reward: {}",
            episode, episode_result.total_reward
        );
    }

    // Get a timestamp to mark our results
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time went backwards")
        .as_secs();

    let mut env = CartPoleEnv::default();

    let render = GifRender::new(540, 540, &format!("img/cart_pole_{}.gif", now), 20).unwrap();

    let result =
        run_episode(&mut agent, &mut env, &mut Some(render)).context("Running test episode")?;

    info!(
        "Model finished the game with a total reward of {}",
        result.total_reward
    );

    Ok(())
}

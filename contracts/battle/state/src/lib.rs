#![no_std]
use battle_io::*;
use gmeta::metawasm;
use gstd::{prelude::*, ActorId};

#[metawasm]
pub trait Metawasm {
    type State = Battle;

    fn player(tmg_id: ActorId, state: Self::State) -> Player {
        state
            .players
            .get(&tmg_id)
            .unwrap_or(&Default::default())
            .clone()
    }

    fn power_and_health(tmg_id: ActorId, state: Self::State) -> (u16, u16) {
        let player = state
            .players
            .get(&tmg_id)
            .unwrap_or(&Default::default())
            .clone();
        (player.power, player.health)
    }

    fn battle_state(state: Self::State) -> BattleState {
        state.state
    }

    fn pairs_for_player(player: ActorId, state: Self::State) -> Vec<PairId> {
        state
            .players_to_pairs
            .get(&player)
            .unwrap_or(&Vec::new())
            .clone()
    }

    fn pair_ids(state: Self::State) -> Vec<PairId> {
        state.pairs.keys().cloned().collect()
    }
    fn current_turn(pair_id: PairId, state: Self::State) -> ActorId {
        if let Some(pair) = state.pairs.get(&pair_id) {
            let current_turn = pair.moves.len();
            return pair.owner_ids[current_turn];
        }
        ActorId::zero()
    }
    fn game_is_over(pair_id: PairId, state: Self::State) -> bool {
        if let Some(pair) = state.pairs.get(&pair_id) {
            return pair.game_is_over;
        }
        true
    }

    fn tmg_ids(state: Self::State) -> Vec<ActorId> {
        state.players_ids
    }

    fn pair(pair_id: PairId, state: Self::State) -> Pair {
        state.pairs.get(&pair_id).unwrap_or(&Default::default()).clone()
    }
}

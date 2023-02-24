#![no_std]

use battle_io::*;
use core::array;
use gstd::{debug, exec, msg, prelude::*, ActorId};
use tmg_io::{TmgAction, TmgEvent};
const MAX_POWER: u16 = 10_000;
const MAX_RANGE: u16 = 7_000;
const MIN_RANGE: u16 = 3_000;
const HEALTH: u16 = 2_500;
const MAX_PARTICIPANTS: u8 = 50;
const MAX_STEPS_IN_ROUND: u8 = 5;
const COLORS: [&str; 6] = ["Green", "Red", "Blue", "Purple", "Orange", "Yellow"];
const TIME_FOR_MOVE: u32 = 60;
const GAS_AMOUNT: u64 = 20_000_000_000;

#[derive(Default, Encode, Decode, TypeInfo)]
pub struct Battle {
    pub admin: ActorId,
    pub players: BTreeMap<ActorId, Player>,
    pub players_ids: Vec<ActorId>,
    pub state: BattleState,
    pub current_winner: ActorId,
    pub pairs: BTreeMap<PairId, Pair>,
    pub players_to_pairs: BTreeMap<ActorId, Vec<PairId>>,
    pub completed_games: u8,
}
static mut BATTLE: Option<Battle> = None;
impl Battle {
    async fn register(&mut self, tmg_id: &TamagotchiId) {
        assert_eq!(
            self.state,
            BattleState::Registration,
            "The game is not in Registration stage"
        );

        assert!(
            self.players_ids.len() < MAX_PARTICIPANTS as usize,
            "Maximum number of players was reached"
        );

        if self.players_ids.contains(tmg_id) {
            panic!("This tamagotchi is already in game!");
        }

        let (owner, name, date_of_birth) = get_tmg_info(tmg_id).await;

        if owner != msg::source() {
            panic!("It is not your Tamagotchi");
        }

        let power = generate_power(*tmg_id);
        let defence = MAX_POWER - power;
        let color_index = get_random_value(COLORS.len() as u8);
        let player = Player {
            owner,
            name,
            date_of_birth,
            tmg_id: *tmg_id,
            defence,
            power,
            health: HEALTH,
            color: COLORS[color_index as usize].to_string(),
            victories: 0,
        };
        self.players.insert(*tmg_id, player);
        self.players_ids.push(*tmg_id);
        if self.players.len() as u8 == MAX_PARTICIPANTS {}
        msg::reply(BattleEvent::Registered { tmg_id: *tmg_id }, 0)
            .expect("Error during a reply `BattleEvent::Registered");
    }

    fn split_into_pairs(&mut self) {
        let mut players_len = self.players_ids.len();

        for pair_id in 0..self.players_ids.len() {
            let first_tmg_id = get_random_value(players_len as u8);
            let first_tmg = self.players_ids.remove(first_tmg_id as usize);
            let first_owner = self
                .players
                .get(&first_tmg)
                .expect("Can't be None: Tmg does not exsit")
                .owner;

            players_len -= 1;

            let second_tmg_id = get_random_value(players_len as u8);
            let second_tmg = self.players_ids.remove(second_tmg_id as usize);
            let second_owner = self
                .players
                .get(&second_tmg)
                .expect("Can't be None: Tmg does not exsit")
                .owner;

            players_len -= 1;

            self.players_to_pairs
                .entry(first_owner)
                .and_modify(|pair_ids| pair_ids.push(pair_id as u8))
                .or_insert_with(|| vec![pair_id as u8]);
            self.players_to_pairs
                .entry(second_owner)
                .and_modify(|pair_ids| pair_ids.push(pair_id as u8))
                .or_insert_with(|| vec![pair_id as u8]);

            let pair = Pair {
                owner_ids: vec![first_owner, second_owner],
                tmg_ids: vec![first_tmg, second_tmg],
                moves: Vec::new(),
                rounds: 0,
                game_is_over: false,
                winner: ActorId::zero(),
                move_deadline: 0,
            };
            self.pairs.insert(pair_id as u8, pair);

            debug!("PLAYERS LEN");
            if players_len == 1 || players_len == 0 {
                break;
            }
        }
    }

    fn start_battle(&mut self) {
        assert!(
            self.completed_games == self.pairs.len() as u8,
            "The previous game is not over"
        );

        self.pairs = BTreeMap::new();
        self.players_to_pairs = BTreeMap::new();
        self.completed_games = 0;
        if self.admin != msg::source() {
            panic!("Only admin can start the battle");
        }

        self.split_into_pairs();

        self.state = BattleState::GameIsOn;

        msg::reply(BattleEvent::BattleStarted, 0)
            .expect("Error in a reply `BattleEvent::BattleStarted`");
    }

    fn make_move(&mut self, pair_id: PairId, tmg_move: Move) {
        let pair_ids = self
            .players_to_pairs
            .get(&msg::source())
            .expect("You have no games");
        if !pair_ids.contains(&pair_id) {
            panic!("It is not your game");
        }
        self.make_move_internal(pair_id, Some(tmg_move));
    }

    fn check_if_move_made(&mut self, pair_id: PairId) {
        debug!("Delayed message");
        assert!(
            msg::source() == exec::program_id() || msg::source() == self.admin,
            "Only program or admin can send this message"
        );
        self.make_move_internal(pair_id, None);
    }

    fn make_move_internal(&mut self, pair_id: PairId, tmg_move: Option<Move>) {
        assert_eq!(
            self.state,
            BattleState::GameIsOn,
            "The game is not in `GameIsOn` state"
        );

        let pairs_len = self.pairs.len();
        let pair = self.pairs.get_mut(&pair_id).expect("Pair does not exist");
        assert_eq!(pair.game_is_over, false, "The game for that pair is over!");

        let current_turn = pair.moves.len();
        let owner = pair.owner_ids[current_turn];

        if tmg_move.is_none() && pair.move_deadline < exec::block_timestamp() {
            panic!("The player's turn time has not yet passed")
        } else {
            assert_eq!(owner, msg::source(), "It is not your turn!");
        }

        pair.moves.push(tmg_move);

        let mut players: Vec<Player> = Vec::new();
        players.push(
            self.players
                .get(&pair.tmg_ids[0])
                .expect("Player does not exist")
                .clone(),
        );
        players.push(
            self.players
                .get(&pair.tmg_ids[1])
                .expect("Player does not exist")
                .clone(),
        );

        if pair.moves.len() == 2 {
            pair.rounds += 1;
            let moves = pair.moves.clone();
            pair.moves = Vec::new();
            let (mut winner, loss_0, loss_1) = resolve_battle(&mut players, moves.clone());
            if pair.rounds == MAX_STEPS_IN_ROUND {
                winner = if players[0].health >= players[1].health {
                    players[1].health = 0;
                    Some(0)
                } else {
                    players[0].health = 0;
                    Some(1)
                };
            }
            if let Some(winner_index) = winner {
                let id = winner_index as usize;
                players[id].victories = players[id].victories.saturating_add(1);
                players[id].power = generate_power(pair.tmg_ids[id]);
                players[id].defence = MAX_POWER - players[id].power;
                players[id].health = 2500;
                let tmg_id = pair.tmg_ids[id];
                self.players_ids.push(tmg_id);
                pair.winner = tmg_id;
                pair.game_is_over = true;
                self.completed_games += 1;
            } else {
                players[0].power = generate_power(pair.tmg_ids[0]);
                players[0].defence = MAX_POWER - players[0].power;
                players[1].power = generate_power(pair.tmg_ids[1]);
                players[1].defence = MAX_POWER - players[1].power;
            }

            self.players.insert(pair.tmg_ids[0], players[0].clone());
            self.players.insert(pair.tmg_ids[1], players[1].clone());

            if self.completed_games == pairs_len as u8 {
                if self.players_ids.len() == 1 {
                    self.state = BattleState::GameIsOver;
                } else {
                    self.state = BattleState::WaitNextRound;
                }
            }

            msg::send(
                self.admin,
                BattleEvent::RoundResult((
                    pair_id,
                    loss_0,
                    loss_1,
                    moves[0].clone(),
                    moves[1].clone(),
                )),
                0,
            )
            .expect("Error in sending a message `TmgEvent::RoundResult`");
        }
        if self.state != BattleState::GameIsOver
            && self.state != BattleState::WaitNextRound
            && !pair.game_is_over
        {
            pair.move_deadline = exec::block_timestamp() + TIME_FOR_MOVE as u64;
            msg::send_with_gas_delayed(
                exec::program_id(),
                BattleAction::CheckIfMoveMade(pair_id),
                GAS_AMOUNT,
                0,
                TIME_FOR_MOVE,
            )
            .expect("Error in sending a delayed message `BattleAction::CheckIfModeMade`");
        }
        msg::reply(BattleEvent::MoveMade, 0)
            .expect("Error in sending a reply `BattleEvent::MoveMade`");
    }

    fn start_new_game_force(&mut self) {
        // assert_eq!(
        //     self.admin,
        //     msg::source(),
        //     "Only admin can force start a new game"
        // );
        // self.current_winner = ActorId::zero();
        // self.players = BTreeMap::new();
        // self.players_ids = Vec::new();
        // self.round = Default::default();
        // self.state = BattleState::Registration;
        // msg::reply(BattleEvent::NewGame, 0).expect("Error during a reply `BattleEvent::NewGame");
    }

    fn update_admin(&mut self, new_admin: &ActorId) {
        assert_eq!(
            self.admin,
            msg::source(),
            "Only admin can update the contract admin"
        );
        self.admin = *new_admin;
        msg::reply(BattleEvent::AdminUpdated, 0)
            .expect("Error during a reply `BattleEvent::AdminUpdated");
    }
}

#[gstd::async_main]
async fn main() {
    let action: BattleAction = msg::load().expect("Unable to decode `BattleAction`");
    let battle = unsafe { BATTLE.get_or_insert(Default::default()) };
    match action {
        BattleAction::Register { tmg_id } => battle.register(&tmg_id).await,
        BattleAction::MakeMove { pair_id, tmg_move } => battle.make_move(pair_id, tmg_move),
        BattleAction::StartBattle => battle.start_battle(),
        BattleAction::StartBattleForce => battle.start_new_game_force(),
        BattleAction::UpdateAdmin(new_admin) => battle.update_admin(&new_admin),
        BattleAction::CheckIfMoveMade(pair_id) => battle.check_if_move_made(pair_id),
    }
}

#[no_mangle]
unsafe extern "C" fn init() {
    let battle = Battle {
        admin: msg::source(),
        ..Default::default()
    };
    BATTLE = Some(battle);
}

pub async fn get_tmg_info(tmg_id: &ActorId) -> (ActorId, String, u64) {
    let reply: TmgEvent = msg::send_for_reply_as(*tmg_id, TmgAction::TmgInfo, 0)
        .expect("Error in sending a message `TmgAction::TmgInfo")
        .await
        .expect("Unable to decode TmgEvent");
    if let TmgEvent::TmgInfo {
        owner,
        name,
        date_of_birth,
    } = reply
    {
        (owner, name, date_of_birth)
    } else {
        panic!("Wrong received message");
    }
}

pub fn get_random_value(range: u8) -> u8 {
    let random_input: [u8; 32] = array::from_fn(|i| i as u8 + 1);
    let (random, _) = exec::random(random_input).expect("Error in getting random number");
    random[0] % range
}

pub fn generate_power(tmg_id: ActorId) -> u16 {
    let random_input: [u8; 32] = tmg_id.into();
    let (random, _) = exec::random(random_input).expect("Error in getting random number");
    let mut random_power = 5000;
    for i in 0..31 {
        let bytes: [u8; 2] = [random[i], random[i + 1]];
        random_power = u16::from_be_bytes(bytes) % MAX_POWER;
        if random_power >= MIN_RANGE && random_power <= MAX_RANGE {
            break;
        }
    }
    random_power
}

#[no_mangle]
extern "C" fn state() {
    let battle = unsafe { BATTLE.get_or_insert(Default::default()) };
    msg::reply(battle, 0).expect("Failed to share state");
}

#[no_mangle]
extern "C" fn metahash() {
    let metahash: [u8; 32] = include!("../.metahash");
    msg::reply(metahash, 0).expect("Failed to share metahash");
}

fn resolve_battle(players: &mut Vec<Player>, moves: Vec<Option<Move>>) -> (Option<u8>, u16, u16) {
    let mut health_loss_0: u16 = 0;
    let mut health_loss_1: u16 = 0;
    let mut winner = None;
    let (winner, loss_0, loss_1) = match moves[..] {
        [Some(Move::Attack), Some(Move::Attack)] => {
            health_loss_1 =
                players[1].health - players[1].health.saturating_sub(players[0].power / 6);
            players[1].health = players[1].health.saturating_sub(players[0].power / 6);

            if players[1].health == 0 {
                winner = Some(0);
            } else {
                health_loss_0 =
                    players[0].health - players[0].health.saturating_sub(players[1].power / 6);
                players[0].health = players[0].health.saturating_sub(players[1].power / 6);
                if players[0].health == 0 {
                    winner = Some(1);
                }
            }
            (winner, health_loss_0, health_loss_1)
        }
        [Some(Move::Attack), Some(Move::Defence)] => {
            let player_0_power = players[0].power.saturating_sub(players[1].defence) / 6;
            health_loss_1 = players[1]
                .health
                .saturating_sub(players[1].health.saturating_sub(player_0_power));
            players[1].health = players[1].health.saturating_sub(player_0_power);
            if players[1].health == 0 {
                winner = Some(0);
            }
            (winner, health_loss_0, health_loss_1)
        }
        [Some(Move::Defence), Some(Move::Attack)] => {
            let player_1_power = players[1].power.saturating_sub(players[0].defence) / 6;
            health_loss_0 = players[0]
                .health
                .saturating_sub(players[0].health.saturating_sub(player_1_power));
            players[0].health = players[0].health.saturating_sub(player_1_power);
            if players[0].health == 0 {
                winner = Some(1);
            }
            (winner, health_loss_0, health_loss_1)
        }
        [Some(Move::Attack), None] => {
            let player_0_power = players[0].power / 6;
            health_loss_1 = players[1]
                .health
                .saturating_sub(players[1].health.saturating_sub(player_0_power));
            players[1].health = players[1].health.saturating_sub(player_0_power);
            if players[1].health == 0 {
                winner = Some(0);
            }
            (winner, health_loss_0, health_loss_1)
        }
        [None, Some(Move::Attack)] => {
            let player_1_power = players[1].power / 6;
            health_loss_0 = players[0]
                .health
                .saturating_sub(players[0].health.saturating_sub(player_1_power));
            players[0].health = players[0].health.saturating_sub(player_1_power);
            if players[0].health == 0 {
                winner = Some(1);
            }
            (winner, health_loss_0, health_loss_1)
        }
        [Some(Move::Defence), Some(Move::Defence)]
        | [None, Some(Move::Defence)]
        | [Some(Move::Defence), None]
        | [None, None] => (winner, health_loss_0, health_loss_1),
        _ => unreachable!(),
    };
    (winner, loss_0, loss_1)
}

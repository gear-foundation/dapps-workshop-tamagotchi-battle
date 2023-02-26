#![no_std]

use battle_io::*;
use gstd::{debug, exec, msg, prelude::*, MessageId, ActorId, ReservationId};
use tmg_io::{TmgAction, TmgEvent};
const MAX_POWER: u16 = 10_000;
const MAX_RANGE: u16 = 7_000;
const MIN_RANGE: u16 = 3_000;
const HEALTH: u16 = 2_500;
const MAX_PARTICIPANTS: u8 = 50;
const MAX_STEPS_IN_ROUND: u8 = 5;
const COLORS: [&str; 6] = ["Green", "Red", "Blue", "Purple", "Orange", "Yellow"];
const TIME_FOR_MOVE: u32 = 60;
const GAS_AMOUNT: u64 = 10_000_000_000;
const RESERVATION_AMOUNT: u64 = 200_000_000_000;
const RESERVATION_DURATION: u32 = 86_400;

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
    pub reservations: BTreeMap<ActorId, ReservationId>,
}
static mut BATTLE: Option<Battle> = None;
impl Battle {
    fn start_registration(&mut self) {
        assert_eq!(
            self.state,
            BattleState::GameIsOver,
            "The previous game must be over"
        );
        self.state = BattleState::Registration;
        self.current_winner = ActorId::zero();
        self.players_ids = Vec::new();
        self.completed_games = 0;
        self.players_to_pairs = BTreeMap::new();
        self.pairs = BTreeMap::new();
        msg::reply(BattleEvent::RegistrationStarted, 0)
            .expect("Error in sending a reply `BattleEvent::RegistrationStarted`");
    }
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
        if !self.players.contains_key(tmg_id) {
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
        } else {
            self.players.entry(*tmg_id).and_modify(|player| player.health = HEALTH);
        }

        self.players_ids.push(*tmg_id);

        let reservation_id = ReservationId::reserve(RESERVATION_AMOUNT, RESERVATION_DURATION)
            .expect("reservation across executions");
        self.reservations.insert(*tmg_id, reservation_id);
        if self.players_ids.len() as u8 == MAX_PARTICIPANTS {
            panic!("Maximum number of participants reached")
        }
        msg::reply(BattleEvent::Registered { tmg_id: *tmg_id }, 0)
            .expect("Error during a reply `BattleEvent::Registered");
    }

    fn split_into_pairs(&mut self) {
        let mut players_len = self.players_ids.len();

        for pair_id in 0..self.players_ids.len() {
            let first_tmg_id = get_random_value(players_len as u8);
            let first_tmg = self.players_ids.remove(first_tmg_id as usize);

            let first_owner = if let Some(player) = self.players.get_mut(&first_tmg) {
                player.health = 2500;
                player.owner
            } else {
                panic!("Can't be None: Tmg does not exsit");
            };

            players_len -= 1;

            let second_tmg_id = get_random_value(players_len as u8);
            let second_tmg = self.players_ids.remove(second_tmg_id as usize);
            let second_owner = if let Some(player) = self.players.get_mut(&second_tmg) {
                player.health = HEALTH;
                player.owner
            } else {
                panic!("Can't be None: Tmg does not exsit");
            };

            players_len -= 1;

            self.players_to_pairs
                .entry(first_owner)
                .and_modify(|pair_ids| pair_ids.push(pair_id as u8))
                .or_insert_with(|| vec![pair_id as u8]);
            self.players_to_pairs
                .entry(second_owner)
                .and_modify(|pair_ids| pair_ids.push(pair_id as u8))
                .or_insert_with(|| vec![pair_id as u8]);
            let msg_id = msg::send_delayed(
                exec::program_id(),
                BattleAction::CheckIfMoveMade {
                    pair_id: pair_id as u8,
                    tmg_id: None,
                },
                0,
                TIME_FOR_MOVE + 1,
            )
            .expect("Error in sending a delayed message `BattleAction::CheckIfModeMade`");
            let pair = Pair {
                owner_ids: vec![first_owner, second_owner],
                tmg_ids: vec![first_tmg, second_tmg],
                moves: Vec::new(),
                rounds: 0,
                game_is_over: false,
                winner: ActorId::zero(),
                move_deadline: exec::block_timestamp() + (TIME_FOR_MOVE * 1_000) as u64,
                msg_id,
            };
            self.pairs.insert(pair_id as u8, pair);

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
        assert_eq!(
            self.state,
            BattleState::GameIsOn,
            "The game is not in `GameIsOn` state"
        );
        let pair_ids = self
            .players_to_pairs
            .get(&msg::source())
            .expect("You have no games");
        if !pair_ids.contains(&pair_id) {
            panic!("It is not your game");
        }
        self.make_move_internal(pair_id, Some(tmg_move));
    }

    fn check_if_move_made(&mut self, pair_id: PairId, tmg_id: Option<TamagotchiId>) {
        debug!("Delayed message");
        assert!(
            msg::source() == exec::program_id() || msg::source() == self.admin,
            "Only program or admin can send this message"
        );
        let pair = self.pairs.get(&pair_id).expect("Pair does not exist");

        // message was sent from reservation
        if let Some(tmg_id) = tmg_id {
            debug!("SAVE GAS FROM RESERVATION {:?}", pair.moves);
            if exec::gas_available() >= GAS_AMOUNT {
                // return gas to previous player
                let reservation_amount = exec::gas_available() - GAS_AMOUNT;
                let reservation_id =
                    ReservationId::reserve(reservation_amount, RESERVATION_DURATION)
                        .expect("Reservation across execution");
                self.reservations.insert(tmg_id, reservation_id);
            }
        }

        if msg::source() == exec::program_id() && pair.msg_id != msg::id() {
            return;
        }

        // if too early for checking the move
        if pair.move_deadline > exec::block_timestamp() {
            debug!("DEADLINE {:?}", pair.move_deadline);
            debug!("TIMESTAMP {:?}", exec::block_timestamp());
            return;
        }

        self.make_move_internal(pair_id, None);
    }

    fn make_move_internal(&mut self, pair_id: PairId, tmg_move: Option<Move>) {
        let pairs_len = self.pairs.len();
        let pair = self.pairs.get_mut(&pair_id).expect("Pair does not exist");
        assert_eq!(pair.game_is_over, false, "The game for that pair is over!");

        let current_turn = pair.moves.len();
        let owner = pair.owner_ids[current_turn];

        debug!("MOVE 10 {:?}", tmg_move);
        debug!("TIME {:?}", exec::block_timestamp());
        debug!("MOVE_DEADLINE {:?}", pair.move_deadline);

        if tmg_move.is_some() {
            assert_eq!(owner, msg::source(), "It is not your turn!");
        } else {
            let current_tmg = pair.tmg_ids[current_turn];
            // Get gas from current player who skips the move
            if let Some(reservation_id) = self.reservations.remove(&current_tmg) {
                debug!("GAS FROM RESERVATION {:?}", current_turn);
                pair.move_deadline = exec::block_timestamp() + (TIME_FOR_MOVE * 1_000) as u64;
                let msg_id = msg::send_delayed_from_reservation(
                    reservation_id,
                    exec::program_id(),
                    BattleAction::CheckIfMoveMade {
                        pair_id,
                        tmg_id: Some(current_tmg),
                    },
                    0,
                    TIME_FOR_MOVE + 1,
                )
                .expect("Error in sending a delayed message `BattleAction::CheckIfModeMade`");
                pair.msg_id = msg_id;
            } else {
                // if player has no reservation that means that he skipped many moves
                // and his initial gas ended
                self.players
                    .entry(current_tmg)
                    .and_modify(|player| player.health = 0);
            }
        };

        pair.moves.push(tmg_move.clone());

        let mut players: Vec<Player> = Vec::new();
        players.push(
            self.players
                .get(&pair.tmg_ids[0])
                .expect("Player does not exist")
                .clone(),
        );
        debug!("GAS  {:?}", exec::gas_available());
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
                let tmg_id = pair.tmg_ids[id];
                self.players_ids.push(tmg_id);
                pair.winner = tmg_id;
                pair.game_is_over = true;
                pair.msg_id = MessageId::from([0; 32]);
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
                    self.current_winner = self.players_ids[0];
                } else {
                    self.state = BattleState::WaitNextRound;
                }
            }
            debug!("GAS  {:?}", exec::gas_available());

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
        debug!("GAS  {:?}", exec::gas_available());
        if self.state != BattleState::GameIsOver
            && self.state != BattleState::WaitNextRound
            && !pair.game_is_over
            && tmg_move.is_some()
        {
            pair.move_deadline = exec::block_timestamp() + (TIME_FOR_MOVE * 1_000) as u64;

            let msg_id = msg::send_with_gas_delayed(
                exec::program_id(),
                BattleAction::CheckIfMoveMade {
                    pair_id,
                    tmg_id: None,
                },
                GAS_AMOUNT,
                0,
                TIME_FOR_MOVE + 1,
            )
            .expect("Error in sending a delayed message `BattleAction::CheckIfModeMade`");
            pair.msg_id = msg_id;
        }
        debug!("PAIR {:?}", pair);
        msg::reply(BattleEvent::MoveMade, 0)
            .expect("Error in sending a reply `BattleEvent::MoveMade`");
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
        BattleAction::StartRegistration => battle.start_registration(),
        BattleAction::Register { tmg_id } => battle.register(&tmg_id).await,
        BattleAction::MakeMove { pair_id, tmg_move } => battle.make_move(pair_id, tmg_move),
        BattleAction::StartBattle => battle.start_battle(),
        BattleAction::UpdateAdmin(new_admin) => battle.update_admin(&new_admin),
        BattleAction::CheckIfMoveMade {
            pair_id,
            tmg_id,
        } => battle.check_if_move_made(pair_id, tmg_id),
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

static mut SEED: u8 = 0;

pub fn get_random_value(range: u8) -> u8 {
    if range == 0 {
        return 0;
    }
    let seed = unsafe { SEED };
    unsafe { SEED = SEED.wrapping_add(1) };
    let random_input: [u8; 32] = [seed; 32];
    let (random, _) = exec::random(random_input).expect("Error in getting random number");
    random[0] % range
}

pub fn generate_damage() -> u16 {
    let seed = unsafe { SEED };
    unsafe { SEED = SEED.wrapping_add(1) };
    let random_input: [u8; 32] = [seed; 32];
    let (random, _) = exec::random(random_input).expect("Error in getting random number");
    let bytes: [u8; 2] = [random[0], random[1]];
    u16::from_be_bytes(bytes) % 500
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
            // penalty for skipping the move
            let penalty = generate_damage();
            health_loss_1 = players[1].health.saturating_sub(
                players[1]
                    .health
                    .saturating_sub(player_0_power)
                    .saturating_sub(penalty),
            );
            players[1].health = players[1]
                .health
                .saturating_sub(player_0_power)
                .saturating_sub(penalty);
            if players[1].health == 0 {
                winner = Some(0);
            }
            (winner, health_loss_0, health_loss_1)
        }
        [None, Some(Move::Attack)] => {
            let player_1_power = players[1].power / 6;
            // penalty for skipping the move
            let penalty = generate_damage();
            health_loss_0 = players[0].health.saturating_sub(
                players[0]
                    .health
                    .saturating_sub(player_1_power)
                    .saturating_sub(penalty),
            );
            players[0].health = players[0]
                .health
                .saturating_sub(player_1_power)
                .saturating_sub(penalty);
            if players[0].health == 0 {
                winner = Some(1);
            }
            (winner, health_loss_0, health_loss_1)
        }
        [None, Some(Move::Defence)] => {
            // penalty for skipping the move
            health_loss_0 = generate_damage();
            players[0].health = players[0].health.saturating_sub(health_loss_0);
            if players[0].health == 0 {
                winner = Some(1);
            }
            (winner, health_loss_0, health_loss_1)
        }
        [Some(Move::Defence), None] => {
            // penalty for skipping the move
            health_loss_1 = generate_damage();
            players[1].health = players[1].health.saturating_sub(health_loss_1);
            if players[1].health == 0 {
                winner = Some(0);
            }
            (winner, health_loss_0, health_loss_1)
        }
        [None, None] => {
            health_loss_0 = generate_damage();
            health_loss_1 = generate_damage();
            players[0].health = players[0].health.saturating_sub(health_loss_0);
            players[1].health = players[1].health.saturating_sub(health_loss_1);
            if players[0].health == 0 {
                winner = Some(1);
            } else if players[1].health == 0 {
                winner = Some(0);
            }
            (winner, health_loss_0, health_loss_1)
        }
        [Some(Move::Defence), Some(Move::Defence)] => (winner, health_loss_0, health_loss_1),
        _ => unreachable!(),
    };
    (winner, loss_0, loss_1)
}

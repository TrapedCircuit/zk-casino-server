use std::{collections::HashMap, str::FromStr};

use aleo_rust::{AleoAPIClient, ProgramManager};
use clap::Parser;
use snarkvm::{
    prelude::{Field, Identifier, Plaintext, PrivateKey, Record, Testnet3, ViewKey},
    synthesizer::{BlockMemory, ConsensusMemory, ConsensusStore, Query, Transaction, VM},
};
use utils::{game_id, game_nonce, shuffle};

use crate::casino::CasinoFilter;

pub mod casino;
pub mod utils;

#[derive(Debug, Parser)]
#[clap(name = "casino-server")]
pub struct Cli {
    #[clap(long)]
    pub pk: String,
    #[clap(long, default_value = "0")]
    pub start_at: u32,
    #[clap(long)]
    pub dest: Option<String>
}

fn main() {
    let cli = Cli::parse();
    tracing_subscriber::fmt::init();
    let api_client = match cli.dest {
        Some(base_url) => AleoAPIClient::new(&base_url, "testnet3").expect("invalid base url"),
        None => AleoAPIClient::local_testnet3("3030"),
    };
    let pk = PrivateKey::<Testnet3>::from_str(&cli.pk).expect("invalid pk");
    let vk = ViewKey::try_from(pk).unwrap();

    let mut record_map = RecordMap::new(pk, vk);

    let program_manager =
        ProgramManager::<Testnet3>::new(Some(pk), None, Some(api_client.clone()), None).unwrap();
    let random_aleo = api_client.get_program("random.aleo").unwrap();
    let start_request = api_client.get_program("start_request.aleo").unwrap();
    let hit_request = api_client.get_program("hit_request.aleo").unwrap();
    let stand_request = api_client.get_program("stand_request.aleo").unwrap();
    let casino = api_client.get_program("zkgaming_blackjack1.aleo").unwrap();
    let all_program = [
        random_aleo,
        start_request,
        hit_request,
        stand_request,
        casino,
    ];
    let rng = &mut rand::thread_rng();
    // Initialize the VM.
    let store = ConsensusStore::<Testnet3, ConsensusMemory<Testnet3>>::open(None)
        .expect("store open failed");
    let vm = VM::from(store).expect("VM initialization failed");
    for p in all_program {
        println!("Adding dependency: {}", p.id());
        let deployment = vm.deploy(&p, rng).expect("deploy failed");
        vm.process()
            .write()
            .finalize_deployment(vm.program_store(), &deployment)
            .expect("finalize deployment failed");
        println!("Added dependency: {}", p.id());
    }

    let query = Query::from(api_client.base_url());
    let mut start_height = cli.start_at;
    loop {
        let query = query.clone();
        start_height = sync(&mut record_map, &api_client, start_height).expect("init failed");

        record_map
            .handle_start_request(&program_manager, &vm, query.clone())
            .expect("handle start request failed");
        record_map
            .handle_combine_and_start(&program_manager, &vm, query.clone())
            .expect("handle hit request failed");
        record_map
            .handle_request_hit(&program_manager, &vm, query.clone())
            .expect("handle hit request failed");
        record_map
            .handle_request_stand(&program_manager, &vm, query.clone())
            .expect("handle stand request failed");

        std::thread::sleep(std::time::Duration::from_secs(10));
    }
}

#[derive(Debug, Clone)]
pub struct RecordMap {
    pub view_key: ViewKey<Testnet3>,
    pub private_key: PrivateKey<Testnet3>,
    pub start_request: HashMap<Field<Testnet3>, (String, Record<Testnet3, Plaintext<Testnet3>>)>,
    pub random_part1: HashMap<Field<Testnet3>, (String, Record<Testnet3, Plaintext<Testnet3>>)>,
    pub random_part2: HashMap<Field<Testnet3>, (String, Record<Testnet3, Plaintext<Testnet3>>)>,
    pub hit_request: HashMap<Field<Testnet3>, (String, Record<Testnet3, Plaintext<Testnet3>>)>,
    pub stand_request: HashMap<Field<Testnet3>, (String, Record<Testnet3, Plaintext<Testnet3>>)>,
    pub game_state: HashMap<Field<Testnet3>, (String, Record<Testnet3, Plaintext<Testnet3>>)>,
}

impl RecordMap {
    fn new(pk: PrivateKey<Testnet3>, vk: ViewKey<Testnet3>) -> Self {
        Self {
            view_key: vk,
            private_key: pk,
            random_part1: HashMap::new(),
            random_part2: HashMap::new(),
            hit_request: HashMap::new(),
            stand_request: HashMap::new(),
            start_request: HashMap::new(),
            game_state: HashMap::new(),
        }
    }
}

pub fn sync(
    record_map: &mut RecordMap,
    client: &AleoAPIClient<Testnet3>,
    start_height: u32,
) -> anyhow::Result<u32> {
    let last_height = client.latest_height()?;
    if start_height >= last_height {
        return Ok(start_height);
    }
    const BLOCKS_PER_REQUEST: u32 = 48;
    let mut end_height = last_height.min(start_height + BLOCKS_PER_REQUEST);
    let mut start_height = start_height;
    while end_height <= last_height {
        tracing::warn!("fetching blocks from {} to {}", start_height, end_height);
        let blocks = client.get_blocks(start_height, end_height)?;

        for b in blocks.into_iter() {
            record_map.filter_in(b.clone())?;
            record_map.filter_out(b)?;
        }
        start_height = end_height;
        end_height = last_height.min(end_height + BLOCKS_PER_REQUEST);
        if start_height >= last_height {
            break;
        }
    }
    tracing::info!("{:#?}", record_map);
    Ok(end_height)
}

impl RecordMap {
    fn handle_start_request(
        &self,
        pm: &ProgramManager<Testnet3>,
        vm: &VM<Testnet3, ConsensusMemory<Testnet3>>,
        query: Query<Testnet3, BlockMemory<Testnet3>>,
    ) -> anyhow::Result<()> {
        for (_, r) in self.start_request.values() {
            let game_id = game_id();
            let game_nonce = game_nonce();
            let inputs = [
                r.clone().to_string(),
                format!("{}u64", game_id),
                format!("{}u64", game_nonce),
            ];
            let rng = &mut rand::thread_rng();
            let tx = Transaction::execute(
                &vm,
                &self.private_key,
                "zkgaming_blackjack1.aleo",
                "process_start_request",
                inputs.iter(),
                None,
                Some(query.clone()),
                rng,
            )?;
            tracing::info!("start a random gen: {}", game_id);
            let result = pm.broadcast_transaction(tx);
            tracing::info!("result: {:?}", result);
        }
        Ok(())
    }
    fn handle_combine_and_start(
        &self,
        pm: &ProgramManager<Testnet3>,
        vm: &VM<Testnet3, ConsensusMemory<Testnet3>>,
        query: Query<Testnet3, BlockMemory<Testnet3>>,
    ) -> anyhow::Result<()> {
        // get random_part1 and random_part2 from same caller
        for (c1, r1) in self.random_part1.values() {
            for (c2, r2) in self.random_part2.values() {
                if c1 == c2 {
                    let nonce1 = r1
                        .data()
                        .get(&Identifier::from_str("nonce").unwrap())
                        .unwrap()
                        .to_string();
                    let nonce2 = r2
                        .data()
                        .get(&Identifier::from_str("nonce").unwrap())
                        .unwrap()
                        .to_string();
                    tracing::info!("accept nonce1: {} and nonce2: {}", nonce1, nonce2);
                    let rng = &mut rand::thread_rng();
                    let nonce1 = u64::from_str(nonce1.trim_end_matches("u64.private")).unwrap_or(game_nonce());
                    let nonce2 = u64::from_str(nonce2.trim_end_matches("u64.private")).unwrap_or(game_nonce());
                    let nonce = nonce1.wrapping_add(nonce2);
                    let cards = shuffle(nonce);
                    let inputs = [
                        r1.clone().to_string(),
                        r2.clone().to_string(),
                        format!("{}u128", cards),
                    ];

                    let tx = Transaction::execute(
                        &vm,
                        &self.private_key,
                        "zkgaming_blackjack1.aleo",
                        "start_game",
                        inputs.iter(),
                        None,
                        Some(query.clone()),
                        rng,
                    )?;
                    tracing::info!("start a game with {c1} {c2} and {nonce1} {nonce2}");
                    let result = pm.broadcast_transaction(tx);
                    tracing::info!("result: {:?}", result);
                }
            }
        }

        Ok(())
    }
    fn handle_request_hit(
        &self,
        pm: &ProgramManager<Testnet3>,
        vm: &VM<Testnet3, ConsensusMemory<Testnet3>>,
        query: Query<Testnet3, BlockMemory<Testnet3>>,
    ) -> anyhow::Result<()> {
        for (caller, r) in self.hit_request.values() {
            let (_, (_, game_state)) = self
                .game_state
                .iter()
                .filter(|(_, v)| v.0 == *caller)
                .next()
                .unwrap();
            let inputs = [game_state.clone().to_string(), r.clone().to_string()];
            let rng = &mut rand::thread_rng();
            let tx = Transaction::execute(
                &vm,
                &self.private_key,
                "zkgaming_blackjack1.aleo",
                "process_hit_request",
                inputs.iter(),
                None,
                Some(query.clone()),
                rng,
            )?;
            tracing::info!("handle a hit request from {}", caller);
            let result = pm.broadcast_transaction(tx);
            tracing::info!("result: {:?}", result);
        }
        Ok(())
    }
    fn handle_request_stand(
        &self,
        pm: &ProgramManager<Testnet3>,
        vm: &VM<Testnet3, ConsensusMemory<Testnet3>>,
        query: Query<Testnet3, BlockMemory<Testnet3>>,
    ) -> anyhow::Result<()> {
        for (caller, r) in self.stand_request.values() {
            let (_, (_, game_state)) = self
                .game_state
                .iter()
                .filter(|(_, v)| v.0 == *caller)
                .next()
                .unwrap();
            let inputs = [game_state.clone().to_string(), r.clone().to_string()];
            let rng = &mut rand::thread_rng();
            let tx = Transaction::execute(
                &vm,
                &self.private_key,
                "zkgaming_blackjack1.aleo",
                "process_stand_request",
                inputs.iter(),
                None,
                Some(query.clone()),
                rng,
            )?;
            tracing::info!("handle a stand request from {}", caller);
            let result = pm.broadcast_transaction(tx);
            tracing::info!("result: {:?}", result);
        }
        Ok(())
    }
}

#[test]
fn test_get_record() {
    let api_client = AleoAPIClient::<Testnet3>::local_testnet3("3030");
    let private_key = PrivateKey::<Testnet3>::from_str(
        "APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH",
    )
    .unwrap();
    let view_key = ViewKey::try_from(private_key).unwrap();
    let records = api_client
        .get_unspent_records(&private_key, 72300..72497, None, None)
        .unwrap();
    let decrypted_records = records
        .into_iter()
        .map(|(_, r)| r.decrypt(&view_key).unwrap())
        .collect::<Vec<_>>();
    println!("{:?}", decrypted_records);
}

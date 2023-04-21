use std::str::FromStr;

use snarkvm::{
    prelude::{Ciphertext, Field, Identifier, Plaintext, PrivateKey, Record, Testnet3, ViewKey},
    synthesizer::{Block, Transition},
};

use crate::RecordMap;

pub trait CasinoFilter {
    fn filter_out(&mut self, block: Block<Testnet3>) -> anyhow::Result<()>;
    fn filter_in(&mut self, block: Block<Testnet3>) -> anyhow::Result<()>;
}

impl CasinoFilter for RecordMap {
    fn filter_out(&mut self, block: Block<Testnet3>) -> anyhow::Result<()> {
        for t in block.into_transitions() {
            match t.function_name().to_string().as_str() {
                "new_start_reqeust" => {
                    let records = get_output_records(t, self.private_key.clone(), self.view_key)?;
                    for (sn, record) in records {
                        let caller = record
                            .data()
                            .get(&Identifier::from_str("signer").unwrap())
                            .unwrap()
                            .to_string();
                        tracing::info!("new start request: {} with record {}", caller, record);
                        self.start_request.insert(sn, (caller, record));
                    }
                }
                "exchange" => {
                    let records = get_output_records(t, self.private_key.clone(), self.view_key)?;
                    for (sn, record) in records {
                        let game_id = record
                            .data()
                            .get(&Identifier::from_str("id").unwrap())
                            .expect("ddd")
                            .to_string();
                        tracing::info!("new exchange request: game_id {} with record {}", game_id, record);
                        self.random_part1.insert(sn, (game_id, record));
                    }
                }
                "set_nonce" => {
                    let records = get_output_records(t, self.private_key.clone(), self.view_key)?;
                    for (sn, record) in records {
                        let caller = record
                            .data()
                            .get(&Identifier::from_str("id").unwrap())
                            .unwrap()
                            .to_string();
                        tracing::info!("new set_nonce request: {} with record {}", caller, record);
                        self.random_part2.insert(sn, (caller, record));
                    }
                }
                "start_game" => {
                    let records = get_output_records(t, self.private_key.clone(), self.view_key)?;
                    for (sn, record) in records {
                        println!("record {:?}", record);
                        let caller = record
                            .data()
                            .get(&Identifier::from_str("player").unwrap())
                            .unwrap()
                            .to_string();
                        tracing::info!("new start_game request: {} with record {}", caller, record);
                        self.game_state.insert(sn, (caller, record));
                    }
                }
                "new_hit_request" => {
                    let records = get_output_records(t, self.private_key.clone(), self.view_key)?;
                    for (sn, record) in records {
                        let caller = record
                            .data()
                            .get(&Identifier::from_str("signer").unwrap())
                            .unwrap()
                            .to_string();
                        tracing::info!("new request_hit request: {} with record {}", caller, record);
                        self.hit_request.insert(sn, (caller, record));
                    }
                }
                "process_hit_request" => {
                    let records = get_output_records(t, self.private_key.clone(), self.view_key)?;
                    for (sn, record) in records {
                        let caller = record
                            .data()
                            .get(&Identifier::from_str("player").unwrap())
                            .unwrap()
                            .to_string();
                        tracing::info!("new process_hit_request request: {} with record {}", caller, record);
                        self.game_state.insert(sn, (caller, record));
                    }
                }
                "new_stand_request" => {
                    let records = get_output_records(t, self.private_key.clone(), self.view_key)?;
                    for (sn, record) in records {
                        let caller = record
                            .data()
                            .get(&Identifier::from_str("signer").unwrap())
                            .unwrap()
                            .to_string();
                        tracing::info!("new request_stand request: {} with record {}", caller, record);
                        self.stand_request.insert(sn, (caller, record));
                    }
                }
                _ => {}
            };
        }
        Ok(())
    }

    fn filter_in(&mut self, block: Block<Testnet3>) -> anyhow::Result<()> {
        block.into_serial_numbers().for_each(|sn| {
            self.start_request.remove(&sn);
            self.random_part1.remove(&sn);
            self.random_part2.remove(&sn);
            self.game_state.remove(&sn);
            self.hit_request.remove(&sn);
            self.stand_request.remove(&sn);
        });
        Ok(())
    }
}

fn get_output_records(
    t: Transition<Testnet3>,
    pk: PrivateKey<Testnet3>,
    vk: ViewKey<Testnet3>,
) -> anyhow::Result<Vec<(Field<Testnet3>, Record<Testnet3, Plaintext<Testnet3>>)>> {
    let mut records = vec![];
    for o in t.outputs().to_owned() {
        if let Some(record) = o.into_record() {
            if record.1.is_owner(&vk) {
                let (commitment, record) = record;
                let sn = Record::<Testnet3, Ciphertext<Testnet3>>::serial_number(pk, commitment)?;
                let record = record.decrypt(&vk)?;
                records.push((sn, record));
            }
        }
    }

    Ok(records)
}

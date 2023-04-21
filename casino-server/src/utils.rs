use rand::Rng;

pub fn shuffle(nonce: u64) -> u128 {
    let mut cards = [0u8; 52];
    for i in 0..52 {
        cards[i] = (i + 1) as u8;
    }

    let mut state = nonce;
    for i in (1..52).rev() {
        let j = state % (i + 1) as u64;
        state = (state * 1103515245 + 12345) % 2u64.pow(31);
        cards.swap(i, j as usize);
    }
    tracing::info!("shuffle cards: {:?}", cards);
    encode(cards.to_vec())
}

pub fn game_id() -> u64 {
    rand::thread_rng().gen_range(0..u64::MAX)
}

pub fn game_nonce() -> u64 {
    rand::thread_rng().gen_range(0..u64::MAX)
}

fn encode(cards: Vec<u8>) -> u128 {
    let mut result = 0u128;
    for i in 0..16 {
        result += (cards[i] as u128) << (i * 8);
    }
    result
}

#[allow(dead_code)]
fn decode(cards: u128) -> Vec<u8> {
    let mut result = vec![0u8; 16];
    for i in 0..16 {
        result[i] = ((cards >> (i * 8)) & 0xff) as u8;
        println!("{} ", result[i] % 13);
    }
    result
}


#[test]
fn test_shuffle_card() {
    let nonce = 2313;
    let cards = shuffle(nonce);
    println!("{}", cards);
    let cards = decode(cards);
    println!("{:?}", cards);
}

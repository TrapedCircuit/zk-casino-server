# CASINO SERVER

## HOW TO START
1. install rust

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

2. build from source
```sh
git clone https://github.com/TrapedCircuit/zk-casino-server
cd casino-server
cargo build --release
```

3. just run it

```sh
./target/release/casino-server --pk 'your_private_key' --start-at 'block start height'
```

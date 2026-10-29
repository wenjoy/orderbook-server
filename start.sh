# 1. 密钥
# solana-keygen new -o /tmp/settlement-authority.json --no-bip39-passphrase --force

# 2. 部署 Program + 初始化 Market/Vault（authority 须与 keypair 一致）

# 3. 启动 API
export SETTLEMENT_MODE=hot_wallet
export SOLANA_RPC_URL=http://127.0.0.1:8899
export SETTLEMENT_PROGRAM_ID=7znWd5GLn6aeay12iovNZWtRC8yau6H5Ee87vDTe4zbz
export SETTLEMENT_MARKET=7znWd5GLn6aeay12iovNZWtRC8yau6H5Ee87vDTe4zbz
export SETTLEMENT_KEYPAIR=~/.config/solana/id.json

cargo run

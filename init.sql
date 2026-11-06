create table markets (
  id BIGSERIAL PRIMARY KEY,
  symbol text not null unique,
  base_symbol text not null,
  quote_symbol text not null,
  base_mint text not null,
  quote_mint text not null
)

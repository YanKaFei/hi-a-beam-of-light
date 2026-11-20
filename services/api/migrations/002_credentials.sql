-- M1: signed usage credentials.
--
-- Nothing here replaces usage_event: legacy rows keep their own table so an old,
-- unsigned claim can never be re-labelled as a verified credential by accident.

create table if not exists device_key (
    id             text primary key,
    identity_id    text   not null references identity(id) on delete cascade,
    key_id         text   not null,
    pubkey_hex     text   not null,
    adapter_id     text,
    scope          text   not null default 'usage'
                            check (scope in ('usage', 'any')),
    status         text   not null default 'ACTIVE'
                            check (status in ('ACTIVE', 'REVOKED')),
    created_at_ms  bigint not null,
    revoked_at_ms  bigint,
    unique (identity_id, key_id)
);

create table if not exists credential (
    id                text primary key,          -- credential_id from the signed payload
    identity_id       text   not null references identity(id) on delete cascade,
    key_id            text   not null,
    protocol_version  text   not null,
    source_type       text   not null,
    source_class      text   not null,           -- derived, see credential.mjs POLICY
    adapter_id        text,
    adapter_version   text,
    model_family      text   not null,
    observation_method text,
    observed_at_ms    bigint not null,
    issued_at_ms      bigint not null,
    transfer_id       text,
    challenge_id      text,
    signature_status  text   not null,           -- VALID | INVALID | UNKNOWN | REVOKED
    payload_sha256    text   not null,
    received_at_ms    bigint not null,
    unique (identity_id, id)
);

create index if not exists credential_identity_idx on credential (identity_id);
create index if not exists credential_transfer_idx on credential (transfer_id);

-- A credential may be consumed exactly once by one relation.
create table if not exists credential_consumption (
    credential_id  text primary key,
    transmission_id text not null references transmission(id) on delete cascade,
    consumed_at_ms bigint not null
);

create table if not exists challenge (
    id                 text primary key,
    identity_id        text   not null references identity(id) on delete cascade,
    purpose            text   not null,
    transfer_id        text   not null references transmission(id) on delete cascade,
    peer_identity_id   text   not null,
    nonce              text   not null,
    issued_at_ms       bigint not null,
    expires_at_ms      bigint not null,
    consumed_at_ms     bigint
);

create index if not exists challenge_transfer_idx on challenge (transfer_id);

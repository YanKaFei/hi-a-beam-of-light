-- FirstLight transmission schema, v1.
--
-- Design constraints that show up directly in this file:
--   * No column anywhere holds chat content, prompts, API keys, or file paths.
--   * Countable usage is enforced at the type level: only E3/E4 can be inserted.
--   * One countable usage row per (identity, family) — a database constraint, not
--     application logic.
--   * One edge per (helper, helped) — the same pair can never inflate a title.
--   * Aggregates deliberately cannot reference an identity (F4 concern, kept here
--     as a comment so nobody "adds it later by accident").
--
-- UUIDs are generated in application code so the schema needs no extensions.

create table if not exists identity (
    id             text primary key,
    anon_id        text        not null unique,
    root_pubkey    text        not null,
    status         text        not null default 'ACTIVE'
                                 check (status in ('ACTIVE', 'REVOKED')),
    created_at_ms  bigint      not null,
    revoked_at_ms  bigint
);

create table if not exists identity_key_epoch (
    identity_id    text   not null references identity(id) on delete cascade,
    epoch          integer not null,
    pubkey         text   not null,
    created_at_ms  bigint not null,
    retired_at_ms  bigint,
    primary key (identity_id, epoch)
);

-- Replay protection: a nonce may be used exactly once.
create table if not exists request_nonce (
    identity_id   text   not null references identity(id) on delete cascade,
    nonce         text   not null,
    seen_at_ms    bigint not null,
    expires_at_ms bigint not null,
    primary key (identity_id, nonce)
);
create index if not exists request_nonce_expiry on request_nonce (expires_at_ms);

create table if not exists idempotency_key (
    key          text primary key,
    identity_id  text   not null references identity(id) on delete cascade,
    endpoint     text   not null,
    request_hash text   not null,
    status_code  integer,
    response     text,
    created_at_ms bigint not null,
    expires_at_ms bigint not null
);

-- A one-time invitation. Only the hash of the token is stored.
create table if not exists transmission (
    id            text primary key,
    initiator_id  text   not null references identity(id) on delete cascade,
    token_hash    text   not null unique,
    channel       text   not null check (channel in ('QR', 'NFC', 'LINK', 'NEARBY')),
    state         text   not null default 'CREATED'
                           check (state in ('CREATED','OFFERED','RECEIVED','ACCEPTED',
                                            'LEARNING','ACTIVATED','ACKNOWLEDGED',
                                            'CANCELLED','EXPIRED','REJECTED','REVOKED')),
    version       integer not null default 1,
    created_at_ms bigint  not null,
    updated_at_ms bigint  not null,
    expires_at_ms bigint  not null,
    check (expires_at_ms > created_at_ms)
);
create index if not exists transmission_initiator on transmission (initiator_id, state);
create index if not exists transmission_expiry on transmission (expires_at_ms);

create table if not exists transmission_party (
    transmission_id text not null references transmission(id) on delete cascade,
    role            text not null check (role in ('INITIATOR', 'RECEIVER')),
    identity_id     text references identity(id),
    confirmed_at_ms bigint,
    primary key (transmission_id, role)
);

-- Countable usage. The CHECK is the F1 rule made physical.
create table if not exists usage_event (
    id              text primary key,
    identity_id     text   not null references identity(id) on delete cascade,
    model_family    text   not null,
    evidence_level  integer not null check (evidence_level in (3, 4)),
    proof_method    text   not null,
    adapter_id      text   not null,
    adapter_version text   not null,
    registry_semver text   not null,
    observed_at_ms  bigint not null,
    received_at_ms  bigint not null,
    unique (identity_id, model_family)
);

create table if not exists light_edge (
    id               text primary key,
    transmission_id  text not null unique references transmission(id) on delete cascade,
    helper_id        text not null references identity(id) on delete cascade,
    helped_id        text not null references identity(id) on delete cascade,
    kind             text not null check (kind in ('FIRST_LIGHT', 'NEW_DISCOVERY', 'RESONANCE')),
    state            text not null check (state in ('FORMED', 'ACTIVATED', 'REVOKED')),
    visibility       text not null default 'PRIVATE'
                        check (visibility in ('PRIVATE', 'ANONYMOUS_PUBLIC')),
    created_at_ms    bigint not null,
    activated_at_ms  bigint,
    acknowledged_at_ms bigint,
    revoked_at_ms    bigint,
    unique (helper_id, helped_id),
    check (helper_id <> helped_id)
);

-- Domain events for SSE. Monotonic id doubles as the SSE Last-Event-ID.
create table if not exists outbox_event (
    id           bigserial primary key,
    kind         text   not null,
    identity_id  text references identity(id) on delete cascade,
    payload      text   not null,
    created_at_ms bigint not null,
    delivered_at_ms bigint
);
create index if not exists outbox_identity on outbox_event (identity_id, id);

create table if not exists consent (
    identity_id    text not null references identity(id) on delete cascade,
    scope          text not null check (scope in ('GLOBAL_FIELD_CONTRIBUTION', 'PUBLIC_LIGHT_PATH')),
    state          text not null check (state in ('GRANTED', 'DECLINED', 'REVOKED')),
    policy_version text not null,
    changed_at_ms  bigint not null,
    primary key (identity_id, scope)
);

create table if not exists audit_log (
    id         bigserial primary key,
    identity_id text,
    action     text not null,
    detail     text,
    at_ms      bigint not null
);

-- Every state transition is recorded so a dispute can be reconstructed without
-- storing any personal content.
create table if not exists transmission_log (
    id              bigserial primary key,
    transmission_id text not null references transmission(id) on delete cascade,
    event           text not null,
    from_state      text not null,
    to_state        text not null,
    actor_id        text,
    at_ms           bigint not null
);
create index if not exists transmission_log_tx on transmission_log (transmission_id, id);


create table if not exists users (
    id            bigserial primary key,
    user_key      text not null unique,
    created_at    timestamptz not null default now(),
    last_login_at timestamptz not null default now()
);

create table if not exists attendance_receipts (
    user_id      bigint not null references users(id) on delete cascade,
    school_date  date not null,
    lecture_key  text not null,
    course_code  text,
    course_name  text not null,
    lecture_time text not null,
    kind         text not null check (kind in ('present', 'late', 'excused')),
    confirmed_at timestamptz not null,
    primary key (user_id, school_date, lecture_key)
);
create index if not exists attendance_receipts_date on attendance_receipts(school_date);

create table if not exists assignment_snapshots (
    user_id          bigint not null references users (id) on delete cascade,
    cmid             bigint not null,
    course_id        bigint not null,
    name             text not null,
    first_due_at     timestamptz,
    first_intro_html text not null default '',
    due_at           timestamptz,
    modified_at      timestamptz,
    first_seen_at    timestamptz not null default now(),
    last_changed_at  timestamptz,
    change_count     integer not null default 0,
    primary key (user_id, cmid)
);

create table if not exists item_checks (
    user_id    bigint not null references users (id) on delete cascade,
    item_key   text not null,
    done       boolean not null,
    updated_at timestamptz not null default now(),
    primary key (user_id, item_key)
);

create table if not exists seat_sessions (
    id           bigserial primary key,
    user_id      bigint not null references users (id) on delete cascade,
    building     text not null,
    room_no      integer not null,
    room_name    text not null,
    seat_no      integer not null,
    period       text not null default 'semester',
    started_at   timestamptz not null,
    start_source text not null,
    expires_at   timestamptz not null,
    extend_count integer not null default 0,
    ended_at     timestamptz,
    end_source   text,
    created_at   timestamptz not null default now()
);
create unique index if not exists seat_sessions_one_active on seat_sessions (user_id) where ended_at is null;

create table if not exists seat_state (
    building    text not null,
    room_no     integer not null,
    seat_no     integer not null,
    state       text not null,
    since       timestamptz not null,
    since_known boolean not null default false,
    primary key (building, room_no, seat_no)
);

create table if not exists seat_events (
    id         bigserial primary key,
    building   text not null,
    room_no    integer not null,
    seat_no    integer not null,
    from_state text,
    to_state   text not null,
    at         timestamptz not null default now()
);
create index if not exists seat_events_room_at on seat_events (building, room_no, at desc);

create table if not exists seat_watch_meta (
    id           integer primary key default 1 check (id = 1),
    last_poll_at timestamptz not null
);

create table if not exists remembered_sessions (
    token_hash text primary key,
    user_id    bigint not null references users (id) on delete cascade,
    nonce      bytea not null,
    ciphertext bytea not null,
    created_at timestamptz not null default now(),
    expires_at timestamptz not null
);
create index if not exists remembered_sessions_user on remembered_sessions (user_id);

create table if not exists todos (
    id         bigserial primary key,
    user_id    bigint not null references users (id) on delete cascade,
    course_id  bigint,
    parent_key text,
    title      text not null check (char_length(title) between 1 and 200),
    note       text not null default '' check (char_length(note) <= 2000),
    due_at     timestamptz,
    all_day    boolean not null default true,
    done_at    timestamptz,
    notify     boolean not null default true,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);
create index if not exists todos_user_due on todos (user_id, due_at);

create table if not exists item_alerts_off (
    user_id    bigint not null references users (id) on delete cascade,
    item_key   text not null,
    created_at timestamptz not null default now(),
    primary key (user_id, item_key)
);

create table if not exists notices_seen (
    user_id bigint not null references users (id) on delete cascade,
    url     text not null,
    seen_at timestamptz not null default now(),
    primary key (user_id, url)
);

create table if not exists background_sessions (
    user_id bigint primary key references users(id) on delete cascade,
    nonce bytea not null, ciphertext bytea not null,
    expires_at timestamptz not null,
    last_poll_at timestamptz,
    next_poll_at timestamptz not null default now(),
    last_error text,
    snapshot jsonb
);
create table if not exists background_devices (
    id text primary key,
    user_id bigint not null references background_sessions(user_id) on delete cascade,
    session_hash text not null,
    expires_at timestamptz not null default now() + interval '14 days'
);
create index if not exists background_devices_user on background_devices(user_id);

create table if not exists notification_devices (
    id text primary key references background_devices(id) on delete cascade,
    user_id bigint not null references users(id) on delete cascade,
    session_hash text not null,
    kind text not null check (kind in ('web','fcm','apns')),
    destination jsonb not null,
    leads integer[] not null,
    seat_leads integer[] not null,
    classroom_alerts boolean not null default false,
    classroom_epoch text not null,
    notice_keys text[],
    expires_at timestamptz not null default now() + interval '90 days',
    last_error text
);
create index if not exists notification_devices_user on notification_devices(user_id);
create table if not exists notification_outbox (
    device_id text not null references notification_devices(id) on delete cascade,
    event_key text not null,
    payload jsonb not null,
    due_at timestamptz not null,
    expires_at timestamptz not null,
    attempts integer not null default 0,
    next_try_at timestamptz not null default now(),
    sent_at timestamptz,
    primary key (device_id, event_key)
);
create index if not exists notification_outbox_due on notification_outbox(due_at) where sent_at is null;

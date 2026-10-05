create table users (
    id            bigserial primary key,
    user_key      text not null unique,
    created_at    timestamptz not null default now(),
    last_login_at timestamptz not null default now()
);

create table account_preferences (
    user_id bigint primary key references users(id) on delete cascade,
    meal_place text not null default 'dorm' check (meal_place in ('dorm', 'staff')),
    timetable_display text not null default 'fit' check (timetable_display in ('full', 'fit')),
    semester_display text not null default 'current' check (semester_display in ('current', 'all')),
    alert_leads integer[] not null default array[60] check (cardinality(alert_leads) <= 5 and alert_leads <@ array[1440, 180, 60, 10, 0]),
    updated_at timestamptz not null default now()
);

create table attendance_receipts (
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
create index attendance_receipts_date on attendance_receipts(school_date);

create table assignment_snapshots (
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

create table item_checks (
    user_id    bigint not null references users (id) on delete cascade,
    item_key   text not null,
    done       boolean not null,
    updated_at timestamptz not null default now(),
    primary key (user_id, item_key)
);

create table seat_sessions (
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
create unique index seat_sessions_one_active on seat_sessions (user_id) where ended_at is null;

create table seat_state (
    building    text not null,
    room_no     integer not null,
    seat_no     integer not null,
    state       text not null,
    since       timestamptz not null,
    since_known boolean not null default false,
    primary key (building, room_no, seat_no)
);

create table seat_events (
    id         bigserial primary key,
    building   text not null,
    room_no    integer not null,
    seat_no    integer not null,
    from_state text,
    to_state   text not null,
    at         timestamptz not null default now()
);
create index seat_events_room_at on seat_events (building, room_no, at desc);

create table seat_watch_meta (
    id           integer primary key default 1 check (id = 1),
    last_poll_at timestamptz not null
);

create table auth_sessions (
    token_hash text primary key,
    family_hash text not null,
    user_id bigint not null references users(id) on delete cascade,
    expires_at timestamptz not null,
    revoked_at timestamptz,
    logout_all_at timestamptz
);
create index auth_sessions_user on auth_sessions(user_id);
create index auth_sessions_family on auth_sessions(family_hash);

create table remembered_sessions (
    token_hash text primary key references auth_sessions(token_hash) on delete cascade,
    user_id    bigint not null references users (id) on delete cascade,
    nonce      bytea not null,
    ciphertext bytea not null,
    created_at timestamptz not null default now(),
    expires_at timestamptz not null
);
create index remembered_sessions_user on remembered_sessions (user_id);

create table todos (
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
    alert_leads integer[] check (cardinality(alert_leads) <= 5 and alert_leads <@ array[1440, 180, 60, 10, 0]),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);
create index todos_user_due on todos (user_id, due_at);

create table item_alerts_off (
    user_id    bigint not null references users (id) on delete cascade,
    item_key   text not null,
    created_at timestamptz not null default now(),
    primary key (user_id, item_key)
);

create table item_alert_leads (
    user_id    bigint not null references users (id) on delete cascade,
    item_key   text not null,
    leads      integer[] not null check (cardinality(leads) <= 5 and leads <@ array[1440, 180, 60, 10, 0]),
    primary key (user_id, item_key)
);

create table notices_seen (
    user_id bigint not null references users (id) on delete cascade,
    url     text not null,
    seen_at timestamptz not null default now(),
    primary key (user_id, url)
);

create table background_sessions (
    user_id bigint primary key references users(id) on delete cascade,
    last_poll_at timestamptz,
    next_poll_at timestamptz not null default now(),
    poll_token text,
    poll_until timestamptz,
    last_error text,
    snapshot jsonb
);
create table background_devices (
    id text primary key,
    user_id bigint not null references background_sessions(user_id) on delete cascade,
    session_hash text not null references auth_sessions(token_hash) on delete cascade,
    nonce bytea not null,
    ciphertext bytea not null,
    expires_at timestamptz not null default now() + interval '14 days',
    unique (id, session_hash)
);
create index background_devices_user on background_devices(user_id);
create index background_devices_login on background_devices(session_hash);

create table notification_devices (
    id text primary key,
    user_id bigint not null references users(id) on delete cascade,
    session_hash text not null,
    kind text not null check (kind in ('web','fcm','apns')),
    destination jsonb not null,
    seat_leads integer[] not null,
    classroom_alerts boolean not null default false,
    classroom_epoch text not null,
    notice_keys text[],
    expires_at timestamptz not null default now() + interval '90 days',
    last_error text,
    foreign key (id, session_hash) references background_devices(id, session_hash) on update cascade on delete cascade
);
create index notification_devices_user on notification_devices(user_id);
create index notification_devices_login on notification_devices(session_hash);
create table notification_outbox (
    device_id text not null references notification_devices(id) on delete cascade,
    event_key text not null,
    payload jsonb not null,
    due_at timestamptz not null,
    expires_at timestamptz not null,
    attempts integer not null default 0,
    next_try_at timestamptz not null default now(),
    sent_at timestamptz,
    claim_token text,
    claim_until timestamptz,
    primary key (device_id, event_key)
);
create index notification_outbox_due on notification_outbox(due_at) where sent_at is null;

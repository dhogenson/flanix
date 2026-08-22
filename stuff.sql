CREATE TABLE GROUPS (
  id UUID not null,
  created_at timestamp not null default NOW(),
  group_id UUID not null,
)

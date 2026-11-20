# M1 数据迁移与回滚

## 迁移

`002_credentials.sql` 只**新增**表：`device_key`、`credential`、`credential_consumption`、`challenge`。
**不修改** `usage_event`、`identity`、`transmission` 的任何列。

因此：

- 旧证据原样保留，不会被批量升级为 v1 凭证。
- 旧记录没有 `source_type`，语义上是 `UNSIGNED_CLAIM`（不参与激活）。
- 回滚 = 关闭 `FL_VERIFIER_V2`。四张新表留在库里成为死数据，不影响旧路径。

## 回滚验证

关闭开关后：

- `/v1/keys/*`、`/v1/challenges`、`/v1/usage/attest-v2` 全部返回 404 `NOT_ENABLED`；
- 旧 `/v1/usage/attest` 行为不变；
- 传光状态机不受影响（未改一行）。

## 不改的东西

`fsm.mjs`、`transmission`/`light_edge` 结构、幂等键、nonce 防重放、SSE 全部原样。
M1 只增加"进入激活前多看一道凭证资质"，没有跳过任何既有检查。

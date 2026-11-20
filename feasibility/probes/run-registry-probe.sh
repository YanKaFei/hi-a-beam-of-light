#!/usr/bin/env bash
# F0 可行性探针：模型注册表归一化
# 用途：证明「naive 去重计数」会把 1 个真实模型家族误判成 9 个。
# 无外部依赖，纯离线，可重复运行。
set -euo pipefail
cd "$(dirname "$0")"

echo "== 真实观测到的 model_provider/model 取值"
echo "   来源：本机 Codex 应用元数据索引（state_5.sqlite -> threads 表的 model 列）"
echo "   本探针只读元数据列，不读取任何对话正文、提示词或凭据。"
echo

printf '%-26s %-10s %s\n' "RAW MODEL STRING" "FAMILY" "NOTE"
printf '%-26s %-10s %s\n' "--------------------------" "----------" "------------------------------"
awk -F'\t' 'NR>1 {printf "%-26s %-10s %s\n", ($1==""?"(empty string)":$1), $2, $3}' probe_registry.tsv
echo

awk -F'\t' 'NR>1 {
  total++
  if ($1 == "") { empty++; fam["UNKNOWN"]++; next }
  nonempty++; fam[$2]++
}
END {
  correct=0
  for (f in fam) if (f != "UNKNOWN") { correct++; printf "  family %-8s <- %d raw value(s)\n", f, fam[f] }
  printf "\n  distinct values in DB (incl. empty) = %d\n", total
  printf "  non-empty raw strings               = %d\n", nonempty
  printf "  NAIVE distinct count                = %d   <- 若直接拿 distinct model 当家族数\n", total
  printf "  CORRECT family count                = %d   <- 查表归一化后的真实家族数\n", correct
  printf "  unused: empty string                = %d   <- 必须不计数\n", empty
  printf "  unused: TASK_LABEL                  = %d   <- codex-auto-review，不是模型家族\n", fam["UNKNOWN"] - empty
  printf "\n  称号后果：naive(%d) -> 拓界者(>=7)     正确(%d) -> 初明者(1)\n", total, correct
  printf "  结论：注册表必须查表 + disposition，不得做字符串推断。\n"
}' probe_registry.tsv

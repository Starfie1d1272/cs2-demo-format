---
"cs2-demo-format": major
---

导出的 schema 升级到 Zod 4，并使用原生 JSON Schema 生成器；消费者需要 Zod 4。Python 参考导出器使用新版解析与数值依赖，最低 Python 版本提升到 3.12。v3 ZIP 文件与字段名称保持一致；整数校验收紧为 JavaScript 安全整数范围，生成的 JSON Schema 同步表达该约束。

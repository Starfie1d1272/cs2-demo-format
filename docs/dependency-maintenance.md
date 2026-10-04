# 依赖维护

包管理器版本以根 `package.json` 的 `packageManager` 为准。云端、本地和 CI 使用同一个精确版本；CI setup action 从 manifest 读取，不另外填写 pnpm 版本。子包命令使用所属 workspace 的包管理器。

日常安装使用 `pnpm install --frozen-lockfile`（workspace 也默认启用冻结安装）。拉取代码后先安装，再运行检查。依赖声明或配置与锁文件不一致时应失败，不能在运行脚本时静默重装。

有意升级时修改依赖声明，运行 `pnpm install --no-frozen-lockfile`，由 pnpm 生成并提交完整锁文件，再执行相关类型检查、测试和构建。pnpm 12 的锁文件包含包管理器 bootstrap 和项目依赖两个 YAML document；不要手工删段、排序或在命令退出时恢复旧锁文件。升级包管理器引起的一次格式转换应与升级一起提交。

验证安装稳定性：保存锁文件 SHA-256，连续执行冻结安装与日常 `pnpm exec` / `pnpm run`，确认哈希不变。需要新增、移除或升级依赖时重新生成锁文件；冻结安装不承担版本更新。

Python 最低版本为 3.12；依赖声明更新后用 `uv lock --upgrade` 生成 `uv.lock`，日常用 `uv sync --locked` 安装。Zod 4 原生 `z.toJSONSchema` 是 JSON Schema 生成入口，运行 `pnpm gen:schema` 并提交生成物。

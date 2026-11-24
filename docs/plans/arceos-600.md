# ArceOS 六题 600 分晋级 计划

> **目标**:在个人作业仓库 `LearningOS/2026a-arceos-aurasuisui` 的 `main` 分支上让六个练习全部通过,`./scripts/total-test.sh` 末行输出 `600`,线上 Actions 评测绿并上传分数,OpenCamp 晋级榜单出现 `aurasuisui=600`。
> **非目标**:(1) 不追求排行榜名次,晋级只看是否 600 满分;(2) 不做 `challenges/` 往期题、不看 `course/*.pptx`;(3) 不改评测链路(`.github/**`、`scripts/**`、`course.json`、`enroll.py`);(4) 不改任何 `Cargo.toml` / `arceos/Cargo.lock`;(5) 不碰项目阶段(stage 7)内容。
> **时点**:基线取证于 2026-11-24 10:30 (Asia/Shanghai),HEAD `31dc1e7`。阶段截止 2026-11-28,剩余 4 天。
> **版本判定**:不发布、不打 tag,无 PATCH/MINOR/MAJOR 变更;唯一对外可见动作是 `git push origin main` 触发评测上传。

## 一、范围与版本

| 改动 | 性质 | 依据 |
|---|---|---|
| `arceos/exercises/*/src/*.rs` 六个练习入口与配套内核组件 | 实现练习 | 仓库 README 的六题表 |
| `arceos/axfs_ramfs/src/*`、`arceos/modules/alt_axalloc/*`、`arceos/modules/bump_allocator/*`、`arceos/ulib/axstd/src/*` | 内核组件补全 | STUDENT_GUIDE 的"从入口跟到组件" |
| `docs/plans/arceos-600.md`(本文件) | 契约 | 本计划 |

## 二、依据

### 2.1 基线

| 项 | 实测值 | 复核方式 |
|---|---|---|
| 工作树 | 干净,HEAD `31dc1e7` | `git status --porcelain` 与 `git rev-parse HEAD` |
| 提交历史 | 1 条(`Initial commit`) | `git log --oneline` |
| 远端同步 | origin/main 已同步,可推送 | `git push --dry-run origin main` |
| 本地六题得分 | 未运行(容器未就绪),按配置应 0/600 | `./scripts/total-test.sh` 末行 |
| Docker daemon | 未运行(`docker info` 无输出) | `docker info --format '{{.ServerVersion}}'` |
| 课程镜像 | 未拉取 | `docker image inspect 1301182193/arceos-env@sha256:6e2cd3cd...` |
| 本机 Rust | 1.97.1,与仓库 `nightly-2024-09-04` 不符 | `rustc --version` |
| 本机 RISC-V 工具链 | 无 qemu、无 riscv64 musl gcc、无 make | `qemu-system-riscv64 --version` 等 |
| WSL | Ubuntu-22.04(Stopped)、docker-desktop(Stopped) | `wsl -l -v` |
| GitHub 变量 | `STUDENT_GITHUB=aurasuisui` | `gh api repos/LearningOS/2026a-arceos-aurasuisui/actions/variables` |
| 线上榜单 | 61 人在榜,57 人 600,本账号不在榜 | OpenCamp `/api/courseRank/getListPager` |

### 2.2 现状与缺口

| # | 要求 | 现状 | 依据 |
|---|---|---|---|
| 1 | 六题全过 600 分才晋级 | 0/600 | 榜单逻辑 score>=totalScore 才算晋级 |
| 2 | 提交到个人仓库 main | 仅 `Initial commit` | `git log` |
| 3 | 每次 push 全量重测、只认最新一次 | 尚无 push 记录 | `.github/workflows/build.yml` |
| 4 | 评测在 ubuntu-24.04 + nightly-2024-09-04 + qemu 8.2.2 | 本机环境不满足 | Dockerfile / build.yml |
| 5 | 不提交产物与日志 | `.gitignore` 已覆盖 `*.img`、`/test.output` | `.gitignore` |

### 2.3 缺陷或风险台账

| ID | 现象与位置 | 关闭条件(可验证) |
|---|---|---|
| **D1** | Docker daemon 未启动,`docker info` 空 | `docker info --format '{{.ServerVersion}}'` 输出非空且 exit 0 |
| **D2** | 课程镜像未拉取(约 2GB,x86_64) | `docker image inspect docker.io/1301182193/arceos-env@sha256:6e2cd3cd...` exit 0 |
| **D3** | 本机 rustc 1.97.1 与仓库 nightly-2024-09-04 冲突 | 容器内 `rustc --version` 含 `nightly-2024-09-04` |
| **D4** | 六题均未实现,得分 0/600 | `./scripts/total-test.sh` 末行 = `600` |
| **D5** | 分数上传依赖 `STUDENT_GITHUB` 与 push 者一致 | Actions `Save measured score and upload to OpenCamp` 为 success,且榜单命中 `aurasuisui` |
| **D6** | 产物(pflash.img/disk.img/target)可能误入版本库 | `git status --porcelain` 为空 |
| **D7** | Docker Hub 拉取慢或失败 | 镜像 inspect exit 0;失败时按风险表走 WSL 兜底并记录 |

## 三、里程碑与出口条件

| # | 里程碑 | 内容 | 出口条件(全部满足) |
|---|---|---|---|
| **M0** | 做题环境与基线 | 启动 Docker Desktop,拉课程镜像,容器内跑一次全量自测确认 0 分起点 | `docker info` exit 0 且 ServerVersion 非空;容器内 `rustc --version` 含 nightly-2024-09-04;容器内 `./scripts/total-test.sh` 末行 `0`,`test.output` 有 6 行 failed |
| **M1** | 第 1 题彩色输出 + 打通推送链路 | 让问候语带 ANSI 颜色并保留 `Hello, Arceos!` 文本;本地过测后 push 一次,确认 Actions 与上传 | 容器内 `./scripts/test-print.sh` 打印 `print_with_color pass` 且 exit 0;`total-test.sh` 末行 >=100;push 后 `2026a-arceos grading` 与上传 job 均 success;榜单 `aurasuisui` 分数 = 100 |
| **M2** | 第 2 题 ramfs 改名 | 在 RAM 文件系统目录节点实现同目录 rename,`/tmp/f1` 改名后能读回内容 | 容器内 `./scripts/test-ramfs_rename.sh` 打印 `ramfs_rename pass` 且 exit 0;`total-test.sh` 末行 >=200 |
| **M3** | 第 3 题替代分配器 | 让练习走替代分配器,使 300 万元素向量分配与排序通过 | 容器内 `./scripts/test-alt_alloc.sh` 打印 `alt_alloc pass` 且 exit 0;`total-test.sh` 末行 >=300 |
| **M4** | 第 4 题 HashMap | 在 axstd 提供 `HashMap`,五万条插入与遍历断言通过 | 容器内 `./scripts/test-support_hashmap.sh` 打印 `support_hashmap pass` 且 exit 0;`total-test.sh` 末行 >=400 |
| **M5** | 第 5 题文件映射 | 补齐用户态程序的文件映射系统调用,能读回 `hello, arceos!` | 容器内 `./scripts/test-sys_map.sh` 打印 `sys_map pass` 且 exit 0;`total-test.sh` 末行 >=500 |
| **M6** | 第 6 题 Hypervisor | 处理客体 VM 退出,使 `skernel2` 正常关机 | 容器内 `./scripts/test-simple_hv.sh` 打印 `simple_hv pass` 且 exit 0;`total-test.sh` 末行 = `600` |
| **M7** | 推送与晋级核对 | 六题全绿后 push 一次,等 Actions 完成并上传,核对榜单 | `gh run list` 最新 grading run conclusion=success 且上传 job success;榜单 `aurasuisui` 分数 = 600;`git status --porcelain` 为空 |

> 判定点:每个里程碑做完,执行者停下报「待审核」;审核者独立复跑后才置 ✅。

## 四、进度看板

| 里程碑 | 状态 | 证据 |
|---|---|---|
| M0 | ⬜ | — |
| M1 | ⬜ | — |
| M2 | ⬜ | — |
| M3 | ⬜ | — |
| M4 | ⬜ | — |
| M5 | ⬜ | — |
| M6 | ⬜ | — |
| M7 | ⬜ | — |

## 五、执行与审核规则

- 一次只派一个里程碑;执行者只报「待审核」,不得改本文件。
- 四件必报:验证命令原样输出 / 测试计数 / commit 列表 / 做法与偏离。
- 审核者独立复跑:用同一条 `docker run ... bash -lc '<cmd>'` 复现,不看执行者截图。
- 每轮审核必查 `git diff --stat` 边界与 `git diff -- '*test*'`,确认没有改测试脚本或放宽断言。
- 不可逆动作:`git push origin main` 会触发全量重测并上传分数,需人类确认后执行(计划内 M1 与 M7 各一次)。
- 禁止清单(所有里程碑通用):`.github/**`、`scripts/**`、`course.json`、`enroll.py`、任意 `Cargo.toml`、`arceos/Cargo.lock`、`docs/STUDENT_GUIDE.md`、`challenges/**`、`course/**`。

## 六、砍尾顺序

**不可砍**:M0(环境)、M1(第一题 + 推送链路验证)、M7(推送与榜单核对)。
**可砍**(按序,砍了要留痕):
1. M4 的通用实现:允许先用 `Vec` 线性查找的极简 `HashMap`,只为过 `test-support_hashmap.sh`;
2. M6 的 vmexit 覆盖面:只处理 `test-simple_hv.sh` 触发的退出码类型;
3. 中间里程碑的额外推送:只保留 M1 与 M7 两次 push;
4. 课件与往期挑战阅读:全部延后到晋级之后。

## 七、风险与对策

| 风险 | 对策 |
|---|---|
| Docker Hub 拉镜像慢/超时 | 先 `docker pull --platform linux/amd64 <image>` 单独重试;仍失败则用 WSL2 Ubuntu-22.04 装 `qemu-system-misc`+`build-essential`+`setup-musl` 兜底,并在台账记录环境差异 |
| 容器内 qemu 跑挂或超时 | 单题用 `timeout 300` 包裹;`make run` 前清理 `pflash.img`/`disk.img`;失败保留完整串口日志再重试 |
| 后续 push 覆盖分数 | 六题全绿前不再 push;最终一次 push 只做 M7 |
| CI `cargo build --locked` 失败 | 全程不改 `Cargo.toml`/`Cargo.lock`;派单里写进禁止清单 |
| 本机 Windows 路径/换行导致脚本异常 | 全部命令在容器 `bash -lc` 内执行,挂载 `-v <repo>:/workspace` |
| 时间不足(4 天) | 按 M1->M2->M3->M4->M5->M6 顺序推进,先拿易分;每完成一题立即审核并记录证据 |

## 八、文档落点

| 改动 | 唯一落点 | 联动 |
|---|---|---|
| 计划与看板 | `docs/plans/arceos-600.md` | 每轮审核后更新看板并单独 commit |
| 六题实现 | `arceos/exercises/<name>/src/*.rs` 及必要内核组件 | 每题一个 commit |
| 提交说明 | commit message(中文,写清题目与做法) | 不用改 README |

## 九、已决与已知债

**已决**:
1. 用官方课程镜像做题,不用本机工具链 —— 本机 rustc 1.97.1、无 qemu、无 riscv64 musl gcc,与 CI 环境不一致。
2. 验收口径 = 容器内 `scripts/test-*.sh` 的 pass 断言,它与 CI `.github/scripts/grade.py` 的 `MESSAGES` 判定同源。
3. 每题独立提交,便于定位回归;分数只看最后一次全量评测。

**已知债**:本题解只面向六题评测断言,不追求通用性(如 HashMap 允许 O(n) 查找);若后续要复用,需另开任务重构。

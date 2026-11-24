# ArceOS 六题 600 分晋级 计划(v2:WSL 原生环境)

> **目标**:在 `LearningOS/2026a-arceos-aurasuisui` 的 `main` 分支上让六个练习全部通过,WSL 内 `./scripts/total-test.sh` 末行输出 `600`,线上 Actions 评测绿并上传分数,OpenCamp 晋级榜单出现 `aurasuisui=600`。
> **非目标**:(1) 不追求榜单名次,晋级只看是否 600;(2) 不做 `challenges/` 与 `course/*.pptx`;(3) 不改评测链路(`.github/**`、`scripts/**`、`course.json`、`enroll.py`);(4) 不改任何 `Cargo.toml` 与 `arceos/Cargo.lock`;(5) 不碰项目阶段(stage 7)。
> **时点**:基线取证于 2026-11-24 10:35 (Asia/Shanghai),HEAD `e99d202`(计划 v1)。阶段截止 2026-11-28,剩 4 天。
> **版本判定**:不发布、不打 tag,无 PATCH/MINOR/MAJOR 变更;对外可见动作只有 `git push origin main`(人类已授权)。

## 一、范围与版本

| 改动 | 性质 | 依据 |
|---|---|---|
| `arceos/exercises/*/src/*.rs` 六个练习入口 | 实现练习 | 仓库 README 六题表 |
| `arceos/modules/axfs/src/root.rs`、`arceos/modules/alt_axalloc/*`、`arceos/modules/bump_allocator/*`、`arceos/ulib/axstd/src/*` | 内核组件补全 | STUDENT_GUIDE 的从入口跟到组件;注意 `arceos/axfs_ramfs/` 是构建外死代码(见 9.已知债) |
| WSL 内工具链(rustup toolchain、musl 交叉编译器、qemu 8.2.2 PATH) | 环境,不进版本库 | Dockerfile 的 pin |
| `docs/plans/arceos-600.md`(本文件) | 契约 | 本计划 |

## 二、依据

### 2.1 基线(WSL 侧 2026-11-24 实测)

| 项 | 实测值 | 复核方式 |
|---|---|---|
| 仓库状态 | 干净,HEAD `e99d202`,main 与 origin 同步 | `git status --porcelain` 与 `git log --oneline -2` |
| Git 身份 / 推送 | `aurasuisui` / `aurasuisui@users.noreply.github.com`,credential helper=manager,`--dry-run` 成功 | `git config --get user.name`、`git push --dry-run origin main` |
| 上传前置 | 仓库变量 `STUDENT_GITHUB=aurasuisui` | `gh api repos/LearningOS/2026a-arceos-aurasuisui/actions/variables` |
| WSL 发行版 | Ubuntu 22.04.5 LTS,内核 6.18.33.2-microsoft-standard-WSL2,x86_64 | `wsl -d Ubuntu-2204 -e bash -lc 'lsb_release -a; uname -r'` |
| WSL 资源 | 16 vCPU,21 GiB 内存,根分区剩余 921 GB,9p 写速约 130 MB/s | `nproc; free -h; df -h ~` |
| sudo | 免密可用 | `sudo -n true` |
| Rust | 登录 shell 有 rustup,已装 1.98.1 与 nightly-2024-05-02,**缺 nightly-2024-09-04** | `rustup toolchain list` |
| rust-objcopy | 已就绪(`~/.cargo/bin/rust-objcopy`) | `command -v rust-objcopy` |
| QEMU | 系统 6.2.0(与 CI 不符);`~/qemu-8.2.2/build/qemu-system-riscv64` 为 8.2.2 自建 | `~/qemu-8.2.2/build/qemu-system-riscv64 --version` |
| RISC-V musl 交叉编译器 | 缺失 | `command -v riscv64-linux-musl-gcc` |
| 构建依赖 | build-essential、pkg-config、zlib1g-dev、dosfstools、mkfs.fat、xxd 有;`mtools`、`libclang-dev`、`libssl-dev` 缺 | `dpkg -s <pkg>` 与 `command -v mcopy` |
| QEMU 加速 | 内核名含 `-microsoft`,Makefile 自动 `ACCEL=n` → `-accel tcg`;`-bios default` 用内置 OpenSBI | `Makefile` 第 101 行、`scripts/make/qemu.mk` |
| 六题得分 | 未运行(工具链未就绪),按配置应为 0/600 | `./scripts/total-test.sh` 末行 |
| 线上榜单 | 61 人在榜、57 人 600,本账号不在榜 | OpenCamp `/api/courseRank/getListPager` |
| Docker | Windows 侧 CLI 29.8.2 已装但 daemon 未起;WSL 内无 docker CLI | `docker info` |

### 2.2 现状与缺口

| # | 要求 | 现状 | 依据 |
|---|---|---|---|
| 1 | 六题全过得 600 才晋级 | 0/600 | 站点 totalScore=600,榜单以 score>=totalScore 计晋级 |
| 2 | 提交到个人仓库 main | 仅 `Initial commit` 与本计划提交 | `git log` |
| 3 | 每次 push 全量重测,只认最新一次 | 无评测记录 | `.github/workflows/build.yml` |
| 4 | 评测环境:ubuntu-24.04 + nightly-2024-09-04 + qemu 8.2.2 + riscv64 musl | 本机缺 toolchain 与 musl,qemu 版本不符 | 2.1 基线 |
| 5 | 产物不入库 | `.gitignore` 已覆盖 `*.img`、`/test.output`、`/tmp/` | `.gitignore` |

### 2.3 缺陷或风险台账

| ID | 现象与位置 | 关闭条件(可验证) |
|---|---|---|
| **D1** | WSL 缺 nightly-2024-09-04,现有 1.98.1 与 nightly-2024-05-02 均与 `rust-toolchain.toml` 不符 | `cd arceos && rustc --version` 输出含 `nightly-2024-09-04` |
| **D2** | 缺 riscv64-linux-musl 交叉编译器,`make payload` 无法构建用户程序 | `command -v riscv64-linux-musl-gcc` 非空,`--version` 首行含 11.2.1 |
| **D3** | 缺 mtools(`mmd`/`mcopy`),`pflash_img`、`disk_img`、`update_disk.sh` 失败 | `command -v mcopy mmd` 均有值 |
| **D4** | 系统 qemu 为 6.2,与 CI 8.2.2 不一致 | `qemu-system-riscv64 --version` 首行含 8.2(PATH 前置自建版本) |
| **D5** | 缺 libclang-dev 与 libssl-dev,构建期可能报找不到 clang/openssl | `dpkg -s libclang-dev libssl-dev` 均为 install ok installed |
| **D6** | 六题均未实现,得分 0/600 | `./scripts/total-test.sh` 末行 = 600 |
| **D7** | 分数上传依赖 `STUDENT_GITHUB` 与 push 者一致 | Actions 的 `Save measured score and upload to OpenCamp` 为 success 且榜单命中 aurasuisui |
| **D8** | 9p 上构建可能过慢 | 单题 `test-*.sh` 端到端 < 8 分钟;超时则启用 ext4 工作副本方案 |
| **D9** | 产物误入版本库 | `git status --porcelain` 为空 |
| **D10** | `scripts/*.sh` 无超时保护,本地第 6 题会永久挂起(实测 315 s 未退),`total-test.sh` 跑不出末行分数;CI 侧由 `grade.py` 的 90 s 兜底 | 本地一律用 `timeout -k 5 300 ./scripts/test-*.sh`;第 6 题返回 124 且无残留 qemu;仓库脚本保持不改(已达成 M0 实测) |

## 三、里程碑与出口条件

测试命令统一执行形式(下称 WSLTEST):

```
wsl.exe -d Ubuntu-2204 -e bash -lc 'cd /mnt/c/Users/aurasui/Desktop/Anything/LearningOS/2026a-arceos-aurasuisui && export PATH=/home/aura/qemu-8.2.2/build:/opt/riscv64-linux-musl-cross/bin:$PATH && <命令>'
```

| # | 里程碑 | 内容 | 出口条件(全部满足) |
|---|---|---|---|
| **M0** | WSL 环境补齐 + 基线 | 装 mtools/libclang-dev/libssl-dev;装 nightly-2024-09-04(含 rust-src、llvm-tools、clippy、rustfmt、riscv64gc-unknown-none-elf);装 musl 交叉编译器到 /opt 并校验 sha256;PATH 前置 qemu 8.2.2;工作树 CRLF 修回 LF;逐题跑基线 | 一、`rustup show active-toolchain` 含 nightly-2024-09-04 二、`riscv64-linux-musl-gcc --version` 首行含 11.2.1 三、`qemu-system-riscv64 --version` 首行含 8.2.2 四、`mcopy -V` 有输出 五、六题逐题 `timeout -k 5 300 ./scripts/test-*.sh` 全部非 0(pass 数 = 0) 六、`git status --porcelain` 为空 |
| **M1** | 第 1 题彩色输出 + 打通推送链路 | 问候语带 ANSI 颜色且保留 `Hello, Arceos!`;过测后 commit 并 push,观察 Actions 与榜单 | WSLTEST `timeout -k 5 300 ./scripts/test-print.sh` 打印 `print_with_color pass` 且 exit 0;六题逐题 pass 数 >=1;最新 run 里 `Test exercises and calculate score` 跑完并给出分数(未满六题时该 job 预期为 failure,属设计内),`Save measured score and upload to OpenCamp` 为 success;榜单 `aurasuisui`=100 |
| **M2** | 第 2 题 ramfs 改名 | RAM 文件系统同目录 rename,`/tmp/f1` 改名后读回内容 | WSLTEST `timeout -k 5 300 ./scripts/test-ramfs_rename.sh` 打印 `ramfs_rename pass` 且 exit 0;六题逐题 pass 数 >=2 |
| **M3** | 第 3 题替代分配器 | 练习走替代分配器,300 万元素向量分配与排序通过 | WSLTEST `timeout -k 5 300 ./scripts/test-alt_alloc.sh` 打印 `alt_alloc pass` 且 exit 0;六题逐题 pass 数 >=3 |
| **M4** | 第 4 题 HashMap | axstd 提供 `HashMap`,五万条插入与遍历断言通过 | WSLTEST `timeout -k 5 300 ./scripts/test-support_hashmap.sh` 打印 `support_hashmap pass` 且 exit 0;六题逐题 pass 数 >=4 |
| **M5** | 第 5 题文件映射 | 补齐文件映射系统调用,用户程序读回 `hello, arceos!` | WSLTEST `timeout -k 5 300 ./scripts/test-sys_map.sh` 打印 `sys_map pass` 且 exit 0;六题逐题 pass 数 >=5 |
| **M6** | 第 6 题 Hypervisor | 处理客体 VM 退出,`skernel2` 正常关机 | WSLTEST `timeout -k 5 300 ./scripts/test-simple_hv.sh` 打印 `simple_hv pass` 且 exit 0;六题逐题 pass 数 = 6 |
| **M7** | 最终 push 与晋级核对 | 六题全绿后 push 一次,等 Actions 上传,核对榜单;可选在 Docker 官方镜像复跑一次 total-test 做平价复核 | `gh run list` 最新 grading success 且上传 job success;榜单 `aurasuisui`=600;`git status --porcelain` 为空;若做 Docker 复核,其末行也是 600 |

> 判定点:每个里程碑做完,执行者停下报待审核;审核者独立复跑后才置 ✅。

## 四、进度看板

| 里程碑 | 状态 | 证据 |
|---|---|---|
| M0 | ✅ | 工具链/交叉编译/qemu/mtools 四项复核通过(rustup active-toolchain=nightly-2024-09-04,musl 11.2.1,qemu 8.2.2,mcopy 4.0.32);六题逐题 timeout 基线全非 0(print 17s, ramfs 159s, alt_alloc 17s, hashmap 13s, sys_map 179s, simple_hv exit124@315s),pass 数 0;git status 空 |
| M1 | ✅ | 本地两次 test-print 均 pass,审核者复跑一次 pass(exit 0);commit 142bbd1 仅改 print_with_color/main.rs(2+/1-),`git diff HEAD~1..HEAD -- '*test*'` 空;push 后 run 45321328396:grade job=failure(未满六题,设计内)、upload job=success(OpenCamp accepted);榜单 rank62 score=100 |
| M2 | ✅ | 审核者复跑 `test-ramfs_rename.sh` → `[Ramfs-Rename]: ok!` + `ramfs_rename pass`,exit 0(149 s);commit 5a01d21 仅改 `modules/axfs/src/root.rs`(+71/-10),`-- '*test*'` 空;第 1 题回归 pass;工作树干净 |
| M3 | ✅ | 审核者复跑 `test-alt_alloc.sh` → `Bump tests run OK!` + `alt_alloc pass`,exit 0(50 s),第 1 题回归 pass(17 s);commit 211bbc7 仅改 `bump_allocator/src/lib.rs`(+159)与 `alt_axalloc/src/lib.rs`(+13),`-- '*test*'` 空;实现为双端 bump,越界返回 NoMemory |
| M4 | ⬜ | — |
| M5 | ⬜ | — |
| M6 | ⬜ | — |
| M7 | ⬜ | — |

## 五、执行与审核规则

- 一次只派一个里程碑;执行者只报待审核,不得改本文件。
- 四件必报:验证命令原样输出 / 测试计数 / commit 列表 / 做法与偏离。
- 审核者独立复跑:用同一段 WSLTEST 自己再跑一遍,不看执行者的截图或转述。
- 本地验收一律加超时:`timeout -k 5 300 ./scripts/test-*.sh`(见 D10);`total-test.sh` 本地不可用,总分按 100 × pass 数自行计算。
- 每轮必查 `git diff --stat` 边界与 `git diff -- '*test*'`,确认未改测试脚本、未放宽断言。
- 提交在 Windows 侧仓库(WSL 侧是同一份文件),push 前后都查 `git status --porcelain`。
- 不可逆动作:`git push origin main` 触发全量重测并上传分数,人类已授权,M1 与 M7 各推一次,其余里程碑不 push。
- 禁止清单(通用):`.github/**`、`scripts/**`、`course.json`、`enroll.py`、任意 `Cargo.toml`、`arceos/Cargo.lock`、`docs/STUDENT_GUIDE.md`、`challenges/**`、`course/**`。

## 六、砍尾顺序

**不可砍**:M0(环境与基线)、M1(第一题 + 推送链路)、M7(最终 push 与榜单核对)。
**可砍**(按序,砍了要留痕):
1. M4 的通用实现:允许 `Vec` 线性查找的极简 HashMap,只为过测试;
2. M6 的 vmexit 覆盖面:只处理测试触发的退出码;
3. M7 的 Docker 平价复核:时间不足就只以 WSL 结果为准;
4. 中间里程碑的额外推送:只保留 M1 与 M7;
5. 课件与往期挑战阅读:延后到晋级之后。

## 七、风险与对策

| 风险 | 对策 |
|---|---|
| rustup 拉 nightly-2024-09-04 慢或断流 | 失败重试一次;仍失败改用 `RUSTUP_DIST_SERVER=https://rsproxy.cn` 镜像并记录 |
| musl 预编译包下载失败 | 用 Dockerfile 同一 URL 与 sha256;失败换镜像前缀重试,校验和不符即丢弃 |
| 9p 构建慢(D8) | 先实测单题耗时;超过 8 分钟则在 WSL ext4 建工作副本(rsync 同步),Windows 侧仍为唯一 git 源 |
| QEMU 6.2 被误用(D4) | 每条命令都 export PATH 前置自建 8.2.2,并在 M0 出口核对版本 |
| 后续 push 覆盖分数 | 六题全绿前不 push;M1 的 100 分只用于验链路 |
| CI cargo build --locked 失败 | 全程不改 Cargo.toml 与 Cargo.lock |
| 时间不足(4 天) | 先易后难(1 到 6),每题完成即审核;11-27 晚必须到 600,11-28 留缓冲 |

## 八、文档落点

| 改动 | 唯一落点 | 联动 |
|---|---|---|
| 计划与看板 | `docs/plans/arceos-600.md` | 每轮审核后更新看板并单独 commit |
| 环境脚本 | WSL 内 `~/.arceos-env.sh`(不进版本库) | 命令统一 source 后执行 |
| 六题实现 | `arceos/exercises/<name>/src/*.rs` 及必要内核组件 | 每题一个 commit,message 中文写明题目与做法 |

## 九、已决与已知债

**已决**:
1. 以 WSL2 Ubuntu-2204 原生环境为主要做题环境——已有 rustup、自建 qemu 8.2.2、16 核与充足磁盘,避免拉取 2GB 镜像;Docker 只做可选平价复核。
2. 验收口径 = WSL 内 `scripts/test-*.sh` 的 pass 断言,与 CI `.github/scripts/grade.py` 的 MESSAGES 同源。
3. qemu 一律用 `~/qemu-8.2.2/build` 前置 PATH,与 CI 的 8.2.2 对齐,不用系统 6.2。
4. 每题独立提交,push 只做 M1 与 M7 两次。
5. 工作树行尾:仓库原以 CRLF 检出(`i/lf w/crlf`),已设 `core.autocrlf=false` 并从 index 还原成 LF,`git status` 为空自证内容零改动;HEAD 中本就有 30 个 `i/crlf` 文件未动。
6. 工具链证据口径:`rustc --version` 由 rustup shim 输出为 `1.83.0-nightly (d6c8169c1 2024-09-03)`(即 nightly-2024-09-04 渠道构建),以 `rustup show active-toolchain` 作为判定命令。

**已知债**:
- 题解只面向六题断言,不追求通用性(如 HashMap 为 O(n));Docker 环境一致性未在本计划闭环,若线上 grading 与本地结论冲突,以线上为准并回退到镜像复核。
- 第 2 题用「拷贝内容 + 删源」兜底,**不保持 inode**;兜底非原子(中途失败留半成品);不支持目录改名;`RootDirectory::rename` 的原生 rename 分支(fatfs)在本套练习里不可达,属未测路径。
- 第 3 题的 EarlyAllocator:页区只增不减(`dealloc_pages` 空实现);`add_memory` 只支持单个连续区间;`axhal::paging`/`axmm`/`axdma` 仍硬编码 `axalloc::global_allocator`,只有 `axruntime`/`axstd` 入口在 `alt_alloc` 下走 `alt_axalloc`(同开 alloc+alt_alloc 会重复定义 `init_allocator`)。
- WSL 侧 git 无 `user.name/user.email`,所有提交在 Windows 侧执行(两侧同一工作树),不改 git 配置。
- `arceos/axfs_ramfs/`(v0.1.1 拷贝)不在构建内:依赖解析到 crates.io 的 `axfs_ramfs 0.1.2`(`.d` 依赖文件指向 registry),只有 `[workspace.dependencies]` 的 path 条目、无 crate 用 `workspace = true`。STUDENT_GUIDE 让学生读它属于课程侧文档与构建不一致,已在 M2 记录,不擅自修。

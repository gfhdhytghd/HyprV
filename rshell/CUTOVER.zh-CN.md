# HyprV:Quickshell → 原生 Rust shell 切换 / 回滚指南

> 适用于当前 `cutover/` 与 `packaging/` 脚本（2026-10-23）。不加 `--apply` 时所有命令都只**打印计划**，不改动任何东西。脚本不会替你调用 sudo；需要 root 的步骤只运行**已暂存、属 root 所有并校验过哈希**的副本，绝不执行本仓库（用户可写）里的文件。
>
> **当前状态：未就绪（NOT READY）。** 像素一致性与性能门槛尚未全绿，`preflight` 不通过时切换脚本会拒绝执行。回滚始终可用。

所有命令都在 `~/.config/HyprV/rshell` 目录下执行。

## 一、准备（只需一次，可反复执行）

1. **构建发布版**
   ```sh
   packaging/build-release.sh
   ```
   最后会打印 64 位的**发布 ID**（`MANIFEST.sha256` 的 SHA-256），请记下。构建期间如果源码有改动，脚本会拒绝生成回执，重跑即可。

2. **跑测试和就绪检查**
   ```sh
   cutover/preflight.sh --run-tests
   ```
   这一步还**不会**显示 `READY`：root 部分尚未安装，`privhelper installed` 一行必然是 `BLOCKED`，这是正常的。要求是：**除 `privhelper installed` 外，其余各行都必须是 `PASS`**（发布版、自动测试、像素一致性、性能）。其中任何一行 `BLOCKED` 都表示还有门槛未通过，不要继续。

3. **安装用户部分**（不需要 root）
   ```sh
   packaging/install-user.sh            # 先看计划
   packaging/install-user.sh --apply
   ```
   安装 `~/.local/bin/hyprv-shell` 和 `~/.config/systemd/user/hyprv-shell.service`。此服务在切换前不会启动。重复执行时输出 `0 file(s) to replace`。

4. **安装 root 部分**
   ```sh
   packaging/install-root.sh            # 用普通用户运行：只打印要执行的命令（已填好 ID）
   ```
   按打印出的命令依次执行，一般如下：
   ```sh
   sudo /usr/bin/install -d -o root -g root -m 0755 /usr/local/lib/hyprv-cutover/releases
   sudo /usr/bin/cp -R -P -T -- packaging/dist/release /usr/local/lib/hyprv-cutover/releases/<ID前16位>
   /usr/bin/sha256sum /usr/local/lib/hyprv-cutover/releases/<ID前16位>/MANIFEST.sha256
   /usr/bin/sh -c 'cd /usr/local/lib/hyprv-cutover/releases/<ID前16位> && /usr/bin/sha256sum --strict --quiet -c MANIFEST.sha256'
   sudo /usr/bin/python3 -I -B /usr/local/lib/hyprv-cutover/releases/<ID前16位>/packaging/install.py root --release <完整ID> --apply
   ```
   - 第 3 条打印的哈希必须**与第 1 步记下的发布 ID 完全一致**，否则停止。第 4 条不应输出任何内容。
   - root 安装会写入：
     - `/usr/libexec/hyprv-privhelper` 与 `hyprv-cooling-failsafe`；
     - helper 的 systemd 单元、D-Bus 激活与策略文件、polkit 策略；
     - 执行 `systemctl mask hyprv-privhelper.service`（仅切换前）；
     - `/var/lib/hyprv-cutover/`（日志与备份）；
     - `/usr/local/lib/hyprv-cutover/installed.json` 和 `current`。
   - root 安装**不会**启动任何服务，也不会动风扇。中途失败会自动恢复旧文件，重复执行没有副作用。

5. **安装后检查**
   ```sh
   cutover/preflight.sh --installed
   ```
   现在必须全部 `PASS`、以 `READY` 结束（包括 `privhelper installed`）。它也会核对已安装版本与你的构建是否一致。2a 会在执行前自动再跑一次同样的检查，不是 `READY` 就拒绝执行。

## 二、切换到原生 shell

前提：第一节第 5 步的 `cutover/preflight.sh --installed` 刚刚以 `READY` 结束。在**图形会话里的终端**中执行，并保持这个终端开着：从 2a 到 2c 之间屏幕上没有任何 bar。

**2a. 停止旧的用户部分**
```sh
cutover/switch.sh --apply --quiesce-legacy
```
预期：记录原有单元状态（含符号链接），停用旧的用户风扇单元，Quickshell 退出，bar 消失。

**2b. root 交接风扇控制**
```sh
sudo /usr/bin/python3 -I -B /usr/local/lib/hyprv-cutover/current/cutover/control.py switch --scope system --apply
```
预期：
- **先**检查 failsafe/helper 能力并成功执行一次 failsafe，再停用任何旧单元；
- 停用 `hyprv-fan-control`、`hyprv-fan-permissions`、`hyprv-cpu-power`；
- 运行最大散热 failsafe，风扇会**短暂全速**；
- 收回用户对 PWM 的写权限，启动 `hyprv-privhelper`；
- 正常结束，退出码 0。

**2b 失败 / 日志卡在 `switching` 的恢复：** 新版脚本会自动恢复日志记录的旧单元及链接，再报告错误。旧版本留下的 `switching` 不要直接继续切换：按第一节步骤 4 暂存、验证并安装本次修复后的 release（此阶段安装不会启动 helper），然后运行：
```sh
sudo /usr/bin/python3 -I -B /usr/local/lib/hyprv-cutover/current/cutover/control.py recover --scope system --apply
```
该命令不要求 preflight 通过，也不启动原生界面；它确认 helper 已停止，恢复原 PWM 权限及旧单元，并核对实际状态。即使 failsafe 再次失败，也会继续恢复旧控制器；恢复不完整则保留原始日志和 `rollback-incomplete`，可重复执行。手动恢复的旧单元不会先被停止。只有确认恢复完整才记为 `legacy`。不要手工删除日志。

**2c. 启动原生 shell**
```sh
cutover/switch.sh --apply
```
预期：
- 应用 hypr 补丁：3 个 Lua 配置和 3 个辅助脚本（`toggle-battery-mode.sh`、`rotate-screen-ccw.sh`、`restore-session-layout.sh`，改为调用 `hyprv-shell desktop …` 与 `hyprv-shell startup-confirm`，不再调用旧 Quickshell 脚本）。修改前先备份到 `~/.local/state/hyprv-cutover/hypr-backup/`；
- 执行 `hyprctl reload`，启动 `hyprv-shell.service`；
- 1 秒左右打印 `# bar mapped on <各显示器>`，退出码 0，bar 重新出现。

**如果失败：** 20 秒内未就绪会**自动回退**：停止原生 shell、撤销补丁、重新载入 Hyprland、重新启动 Quickshell，退出码 1，状态为 `fallback`。此时风扇仍由 helper 控制。修好问题后重新执行 2c，或按第四节回滚。

## 三、验证

```sh
~/.local/bin/hyprv-shell ipc call bar dataStatus        # 输出 JSON 即 IPC 正常
systemctl --user status hyprv-shell.service             # active (running)
systemctl status hyprv-privhelper.service               # active (running)
python3 -c 'import json;print(json.load(open("'$HOME'/.local/state/hyprv-cutover/state.json"))["phase"])'   # native
python3 -c 'import json,subprocess as s;pid=int(s.check_output(["systemctl","--user","show","-p","MainPID","--value","hyprv-shell.service"]));L=json.loads(s.check_output(["hyprctl","-j","layers"]));print({m:any(l["namespace"]=="hyprv-quickshell" and l["pid"]==pid for lv in v["levels"].values() for l in lv) for m,v in L.items()})'
```

最后一条应对每个显示器都输出 `True`，表示 bar 由原生进程绘制。

补丁后的脚本可以这样验证：
```sh
~/.config/HyprV/hypr/scripts/toggle-battery-mode.sh   # 再执行一次切回；~/.local/state/hyprv/quickshell/ui-state.env 中的 variant 在 bt 与 v2 之间切换
~/.local/bin/hyprv-shell desktop deskflowReflow         # Deskflow 配置正常时无输出，返回 0；shell 未运行时会直接执行
```

原生 shell 不依赖系统总线也能运行：系统 D-Bus 不可用时，相关服务显示为不可用，界面照常工作；总线恢复后 shell 会自动重启自身（PID 不变）以重新连接。也可以手动试：SUPER+A 打开原生控制中心，亮度键弹出原生 OSD，SUPER+SHIFT+R 重启原生 shell。

## 四、回滚到 Quickshell

不论门槛是否通过都可以回滚。按顺序执行：

**4a. 停止原生 shell**
```sh
cutover/rollback.sh --apply --quiesce-native
```
预期：删除自启动标记，停止原生 shell，bar 消失。即使你改过 hypr 配置，这一步也不会被阻止。

**4b. root 交还风扇控制**
```sh
sudo /usr/bin/python3 -I -B /usr/local/lib/hyprv-cutover/current/cutover/control.py rollback --scope system --apply
```
预期：
- 停止并 mask helper，运行 failsafe（风扇短暂全速），**确认 helper 已停止**；
- 恢复原 PWM 属主/权限，以及旧系统单元的原始链接和运行状态（权限单元在前）；
- 退出码 0。

**4c. 恢复配置和界面**
```sh
cutover/rollback.sh --apply
```
预期：
- 撤销 hypr 补丁（6 个文件，文件权限保持不变）。如果之后你改过这些文件，会按内容只还原被补丁改过的行，保留你的改动；无法自动还原的行会列出来，并给出备份路径。
- `hyprctl reload`，恢复用户单元，重新启动 Quickshell，bar 恢复。

**看退出码：**

| 结果 | 含义 / 处理 |
|---|---|
| 0，状态 `legacy` | 回滚完成。再执行一次会提示 `No pending user cutover`。 |
| 2，状态 `rollback-incomplete` | 有未完成项，界面已恢复。按提示处理后**重复同一条命令**，直到返回 0。 |
| 状态 `ui-restored` | 跳过了 4b：先执行 4b，再执行 4c。 |
| 状态 `fallback-blocked` | 原生 shell 无法停止：手动停止后重跑。 |

回滚不会删除已安装的文件，留待以后再次切换。三个 root 操作（安装、切换、回滚）共用一把锁，不会同时执行。

## 五、附注

- 整套用户级流程已在隔离的嵌套 Hyprland 中真实演练，包括两种故障自动回退、不完整回滚后的重试，以及回滚后配置逐字节恢复；root 步骤在演练中只生成计划，结果为模拟。详见 [docs/verify/cutover-rehearsal.md](docs/verify/cutover-rehearsal.md)。
- 更完整的说明（架构、QUIRK 开关、已知差异）见 [README.md](README.md) 和 [cutover/README.md](cutover/README.md)。

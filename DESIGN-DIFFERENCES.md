# SuperPower 与 XPS：设计差异

记录日期：2026-11-06。范围：HyprV 配置及本次 XPS Rust 桌面栏改动。
本文区分共同约定、主机设计和暂存的实现差异。保留差异不表示所有差异均为长期设计。
不得用另一台机器的整目录覆盖本机配置。

## 本次共同约定

- 全屏：优先调用 Hymission；插件不可用时调用 `hl.dsp.window.fullscreen`。
- 截图：优先调用 Hyprcapture；插件不可用时调用 `grim | swappy`，区域由 `slurp` 选择。
- 托盘：等待 `org.kde.StatusNotifierWatcher`，最多 30 秒，然后启动托盘应用。
- Quickshell 日历：按需实例化日历面板和 CalendarBridge。两机实际桌面服务使用 Rust shell；其日历由 Rust 实现控制。
- 通知：使用共同的浅色/深色 CSS 和 SwayNC layer blur 规则。
- Quickshell 重载：按用户及配置路径筛选实例，先 TERM，0.2 秒后 KILL，再启动。保留主机各自的渲染和风扇启动设置。

## 主机设计

| 项目 | SuperPower | XPS |
|---|---|---|
| 分支基线 | `main` / `82cf9b0`，含未提交改动 | `laptop` / `cf6fcf5`，含未提交改动 |
| 邮件 / 飞书宽度 | Thunderbird 0.50；飞书 0.50 | Thunderbird 0.85；飞书 0.75 |
| Win+B | 无绑定；不切换省电模式 | 保留日常/省电模式切换 |
| 省电模式 | 保留配置文件，但不提供 Win+B 入口 | 窗口圆角 0；关闭屏幕四角遮罩；栏锁定 minimal；顶部栏预留 15，窗口外边距四边均为 2 逻辑像素 |
| 电源配置 | 本次不改 | 进入省电模式或登录该模式时设置一次 `power-saver`。随后允许手动改为 balanced/performance，不轮询、不锁定。退出模式保留当前电源配置 |
| 显示器 | DP-4 LG 6K；Moonlight/Viewflow 虚拟屏幕 | eDP-1 OLED；LG 6K 外接屏位于内屏上方；其他外屏自动配置 |
| 亮度 | DDC；主屏参数 desktop=173 / peak=446；低亮度调整 SDR 白点 | 内屏背光 desktop=153 / peak=400；与外接 LG 同步；自动亮度及插拔同步 |
| 显示器管理 | 保存主屏旋转；桌面显示器电源逻辑 | 合盖与 LG 输入守护脚本；内屏缺席时保留亮度请求 |
| HDR 色调映射 | Chromium/Electron `limited` | Chromium/Electron `clamp` |
| Sunshine | NVIDIA 环境变量 | 默认 Intel 环境 |
| 自动启动 | QQ、微信、Telegram、Discord 自动启动与排列 | 四者手动启动；Thunderbird/飞书顺序启动；Code OSS 宽度 0.90（桌面 0.85） |
| 窗口规则 | 桌面布局 | Dolphin 居中，宽 50% / 高 70%；LazyCat 与 LAIS 在工作区 1 |
| Hymission | Stage 默认开启；工作区分块及对应快捷键 | Stage 默认关闭；Super+逗号、四指手势切换；边缘进入/退出及顶部栏切换 |
| 语音 | Menu / Alt+Menu | Copilot / Alt+Copilot |
| 其他快捷键 | Super+Shift+F23 侧栏 | 该键用于语音；保留 Super+C 日历和 Super+三指竖滑日历 |
| 指纹 | 无指纹配置 | Hyprlock 指纹配置 |
| 控制面板 | Windows VM、Viewflow、风扇控制 | 笔记本电池、电源配置；Dell 固件管理散热 |
| 栏 | 桌面完整栏 | 日常模式允许 minimal/full 切换；省电模式拒绝切换，退出后恢复用户此前选择 |
| 渲染启动 | Quickshell `QSG_RENDER_LOOP=basic`，恢复风扇 | 保留默认渲染循环，不恢复桌面风扇 |
| SwayNC 系统服务 | 本次不修改服务环境 | 既有 `blur-compat.conf` 禁用 GTK 背景效果协议；CSS 同步不删除此适配 |

省电模式是 HyprV 桌面模式，不等同于当前电源配置，也不等同于当前是否使用电池供电。
minimal 栏自身占 15 逻辑像素；其下的窗口与工作区边缘之间保持 2 像素间距。
窗口顶部相对物理屏幕边缘为 17 像素；左右和底部为 2 像素。

## 暂存的实现差异

以下实现保留各机版本，本次不合并，也不认定为长期主机需求。

- Quickshell Agent：SuperPower 只读任务展示与聚焦；XPS 含审批/拒绝/回答流程及对应资源。
- Quickshell 日历事件布局：SuperPower 按重叠事件簇分列；XPS 按整天的最大重叠数分列。此次只同步加载方式。
- Quickshell 动画、音频频谱、资源图表、滑块和数据订阅不同。
- SuperPower 的 WindowsVmController、desktop-state、StreamCommand 和风扇服务安装文件保留本机。
- 两机旋转、显示器电源、亮度 worker 和模式切换脚本保留硬件适配。
- 本次未全面比较 `rshell`。XPS 栏策略从 XPS 自身源码快照修改，不从 SuperPower 未提交源码迁移。

## 验证与应用范围

静态检查包括 Lua、Shell、嵌入 Python、Qt 6 QML 语法；Rust 测试检查最简栏锁定、解除锁定、四角遮罩和顶部预留。
电源切换使用临时 HOME 与模拟命令测试，验证进入时只设置一次、退出时保留手动配置。
运行检查及应用结果另见本次操作报告。视觉效果必须与语法/模型测试分开表述。

初始运行版本：SuperPower Hyprland `55ce6194`，XPS `f6725e43`。
两端均使用 Lua 配置。版本及运行模式是本次观察，不作为长期固定设计。

## 保留差异文件清单

下列清单按本次同步后的文件内容生成，不包含备份、壁纸和完整 Rust 源码树。
`内容不同` 表示文件内仍有上述主机适配或暂存实现差异。

- 内容不同：`hypr/brightness-hdr.lua`
- 仅 XPS：`hypr/brightness-sync.lua`
- 内容不同：`hypr/controls-profile.lua`
- 内容不同：`hypr/controls.lua`
- 内容不同：`hypr/hyprland-battery.lua`
- 内容不同：`hypr/hyprland-bind.lua`
- 内容不同：`hypr/hyprland-dropitem.lua`
- 内容不同：`hypr/hyprland-exec-once.lua`
- 内容不同：`hypr/hyprland-main.lua`
- 内容不同：`hypr/hyprland-monitors.lua`
- 内容不同：`hypr/hyprland-plugins.lua`
- 内容不同：`hypr/hyprland-winrule.lua`
- 内容不同：`hypr/hyprlock.conf`
- 内容不同：`hypr/lais-voice.lua`
- 内容不同：`hypr/scripts/control-worker.lua`
- 仅 XPS：`hypr/scripts/lg-input-guard.py`
- 仅 XPS：`hypr/scripts/lid-monitor-guard.py`
- 内容不同：`hypr/scripts/restore-session-layout.sh`
- 内容不同：`hypr/scripts/rotate-screen-ccw.sh`
- 内容不同：`hypr/scripts/toggle-battery-mode.sh`
- 内容不同：`hypr/scripts/toggle-monitor-power.sh`
- 仅 SuperPower：`quickshell/AgentTaskModel.qml`
- 内容不同：`quickshell/AnimatedGlassPanel.qml`
- 内容不同：`quickshell/AudioSpectrum.qml`
- 内容不同：`quickshell/ControlPanelPopup.qml`
- 内容不同：`quickshell/ControlPanelSlider.qml`
- 内容不同：`quickshell/DynamicIsland.qml`
- 仅 XPS：`quickshell/LaptopBatteryCard.qml`
- 仅 SuperPower：`quickshell/StreamCommand.qml`
- 内容不同：`quickshell/SystemStatsPopup.qml`
- 内容不同：`quickshell/SystemTrendChart.qml`
- 内容不同：`quickshell/WeekCalendarPanel.qml`
- 内容不同：`quickshell/WindowsVmControl.qml`
- 仅 SuperPower：`quickshell/WindowsVmController.qml`
- 仅 XPS：`quickshell/assets/lais/bubble.svg`
- 仅 XPS：`quickshell/assets/lais/chip.svg`
- 仅 XPS：`quickshell/assets/lais/lais.png`
- 仅 XPS：`quickshell/assets/lais/layers.svg`
- 仅 XPS：`quickshell/assets/lais/nodes.svg`
- 仅 XPS：`quickshell/assets/lais/orbit.svg`
- 仅 XPS：`quickshell/assets/lais/preview.html`
- 仅 XPS：`quickshell/assets/lais/spark.svg`
- 内容不同：`quickshell/scripts/agent-island-action.py`
- 内容不同：`quickshell/scripts/agent-island-hook.py`
- 内容不同：`quickshell/scripts/agent-island-install-hooks.py`
- 内容不同：`quickshell/scripts/agent-island-status.py`
- 内容不同：`quickshell/scripts/agent_island.py`
- 内容不同：`quickshell/scripts/control-panel-status.sh`
- 仅 SuperPower：`quickshell/scripts/desktop-state.py`
- 仅 SuperPower：`quickshell/scripts/fan-control-boot.service`
- 内容不同：`quickshell/scripts/fan-control-helper.sh`
- 仅 SuperPower：`quickshell/scripts/fan-control-permissions-early.conf`
- 仅 SuperPower：`quickshell/scripts/fan-control.service`
- 仅 SuperPower：`quickshell/scripts/install-fan-control-boot.sh`
- 内容不同：`quickshell/scripts/launch.sh`
- 内容不同：`quickshell/scripts/reload.sh`
- 内容不同：`quickshell/shell.qml`
- 仅 SuperPower：`quickshell/tests/fixtures/quickshell/scripts/windows-vm-control.sh`
- 仅 SuperPower：`quickshell/tests/test_agent_readonly.py`
- 仅 SuperPower：`quickshell/tests/test_desktop_state.py`
- 仅 SuperPower：`quickshell/tests/test_fan_control.py`
- 仅 SuperPower：`quickshell/tests/test_windows_vm.py`
- 仅 SuperPower：`quickshell/tests/tst_AgentTaskModel.qml`
- 仅 SuperPower：`quickshell/tests/windows-vm-lifecycle.qml`

## 2026-11-06：XPS 控制中心与托盘

- 省电桌面模式下，Rust 控制中心主页只将各模块自身背景设为不透明；模块接缝保持完全透明，不绘制整块底板。蓝牙页和代理页作为独立面板使用不透明背景。
- minimal 栏模式下，控制中心主页顶部显示系统托盘。日常模式的 minimal 栏也显示此区。
- 完整栏模式下，控制中心不重复显示托盘区。
- 托盘区使用与激活模块一致的浅蓝色背景。托盘图标每行最多 8 个，超出后换行。左键沿用应用激活行为，右键沿用托盘菜单行为。
- 屏幕四角遮罩在初始绘制、统计刷新和常规刷新路径均受省电模式约束。
- 本节仅改变 XPS Rust 桌面栏；SuperPower 的控制中心策略不变。

### 省电窗口透明与配色补充

- XPS 省电模式通过 `battery-windows-solid` 窗口规则启用 `force_rgbx`，忽略应用窗口自身的透明通道。全局 active/inactive/fullscreen opacity 均为 1。此规则不作用于 layer-shell，控制中心模块接缝保持透明。
- 控制中心采用 Claude 提供的配色：浅色激活模块与托盘 `#D6E4F7`、普通模块 `#F4F6FA`、主文字 `#14213D`、次文字 `#4A5670`、激活图标和文字 `#1B3A6B`、边框 `#A9BCD9`。
- 深色对应颜色为 `#2E4A6E`、`#1C2230`、`#E8EEF7`、`#B3C0D4`、`#EAF2FF`、`#4C6690`。配色仅作用于省电控制中心内容。
- 托盘不显示标题，不保留标题行空间。单行高度 40 逻辑像素；空托盘不占空间。

### XPS minimal 实际边距校正

2026-11-06：当前 Hyprland `f6725e43` 的 scrolling 布局下，`gaps_out=2` 实测产生左右 3、上下 2 逻辑像素。省电配置改用 `{ top=2, right=1, bottom=2, left=1 }` 补偿水平偏移。重载后当前窗口位置 `[2,17]`、尺寸 `[1436,881]`，屏幕逻辑尺寸 `1440x900`，扣除顶部栏 15 后四边均为 2。未切换其他工作区；它们的窗口位置等待布局重新计算。

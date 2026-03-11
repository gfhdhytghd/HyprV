################################################################################
# hycov.sh
# 用于在特殊工作区（mission_control）中记录窗口信息，并在恢复时重新布局
Author:Wilf Lin
################################################################################
windows_number=$(hyprctl clients -j | jq 'length')

# Debug mode: 0=off, 1=debug only (no execution), 2=debug + execution
HYCOV_DEBUG=${HYCOV_DEBUG:-0}
dbg() { [[ "$HYCOV_DEBUG" = "1" || "$HYCOV_DEBUG" = "2" ]] && echo "[DEBUG] $*"; }
hy_dispatch() {
  if [[ "$HYCOV_DEBUG" = "1" ]]; then
    echo "hyprctl dispatch $*"
    return 0
  elif [[ "$HYCOV_DEBUG" = "2" ]]; then
    echo "[DEBUG] hyprctl dispatch $*"
    hyprctl dispatch "$@"
  else
    hyprctl dispatch "$@"
  fi
}

get_cursor_position() {
    cursor_x=$(hyprctl cursorpos -j | jq -r '.x')
    cursor_y=$(hyprctl cursorpos -j | jq -r '.y')
    dbg "get_cursor_position: x=${cursor_x} y=${cursor_y}"
}

get_cursor_monitor_wh() {
  get_cursor_monitor_geom
  : # 现在已导出 curr_monitor_width/curr_monitor_height
}

# 返回当前鼠标所在显示器的 ID（纯数字）
get_cursor_monitor_id() {
  # 把 x y 挤成一行，避免 read 分行混乱
  read -r mx my < <(hyprctl cursorpos -j | jq -r '"\(.x) \(.y)"')

  hyprctl monitors -j | jq -r --argjson mx "$mx" --argjson my "$my" '
    .[]
    | select(
        ($mx >= .x) and ($mx <= (.x + .width  - 1)) and
        ($my >= .y) and ($my <= (.y + .height - 1))
      )
    | .id
  ' | head -n1
}
get_cursor_monitor_geom() {
# 读取光标全局（布局坐标，已考虑 Hyprland 的逻辑坐标体系）
read -r cx cy < <(hyprctl cursorpos -j | jq -r '"\(.x) \(.y)"')


# 命中光标所在的显示器，并导出：原始宽高 + scale；随后导出“按缩放修正后的宽高”
eval "$(hyprctl monitors -j | jq -r --argjson mx "$cx" --argjson my "$cy" '
.[]
| select($mx >= .x and $mx < (.x + .width) and $my >= .y and $my < (.y + .height))
| "curr_monitor_id=\(.id) curr_monitor_x=\(.x) curr_monitor_y=\(.y) curr_monitor_width_raw=\(.width) curr_monitor_height_raw=\(.height) curr_monitor_scale=\(.scale // 1) curr_monitor_name=\(.name)"')"


# 兜底：某些版本可能没有 .scale 字段
: "${curr_monitor_scale:=1}"


# 计算按缩放修正后的宽高（逻辑尺寸）。保留原始尺寸在 *_raw 变量中。
curr_monitor_width=$(awk -v w="${curr_monitor_width_raw}" -v s="${curr_monitor_scale}" 'BEGIN{ if(s==0) s=1; printf "%d", int(w/s) }')
curr_monitor_height=$(awk -v h="${curr_monitor_height_raw}" -v s="${curr_monitor_scale}" 'BEGIN{ if(s==0) s=1; printf "%d", int(h/s) }')


dbg "get_cursor_monitor_geom: id=${curr_monitor_id} name=${curr_monitor_name} x=${curr_monitor_x} y=${curr_monitor_y} w=${curr_monitor_width}(raw=${curr_monitor_width_raw}) h=${curr_monitor_height}(raw=${curr_monitor_height_raw}) scale=${curr_monitor_scale}"
}
ensure_special_on_monitor() {
  local mon_id="$1" name="$2"
  # 读到目标显示器的 name
  local mon_name; mon_name="$(hyprctl monitors -j | jq -r --argjson id "$mon_id" '.[]|select(.id==$id)|.name')"
  # 是否已是该 special
  local curr; curr="$(hyprctl monitors -j | jq -r --argjson id "$mon_id" '.[]|select(.id==$id)|.specialWorkspace.name')"
  if [[ "$curr" != "$name" ]]; then
    dbg "ensure_special_on_monitor: id=${mon_id} name=${mon_name} switch_to=${name} curr=${curr}"
    hy_dispatch focusmonitor "$mon_name" || true
    hy_dispatch togglespecialworkspace "$name" || true
  fi
}

# Core layout generator
# mission_control_layout WIDTH HEIGHT COUNT [--margin T R B L] [--padding T R B L] [--bases "2x4,3x5,..."] [--force "7:4,3;3:2,1;9:3,3,3"]
# 说明：
# - 外边距：使用 --margin 参数，可提供1个或4个值，默认20
# - 内边距：使用 --padding 参数，可提供1个或4个值，默认5
# - --bases：逗号分隔的 rxc 列表；每个 rxc 还会自动加入 (r-1)x(c+1) 作为候选（若 r>1）
# - --force：分号分隔的映射；形如 N:row1,row2,...    例：7:4,3  或 9:3,3,3
#   命中 N 时按各行列数严格布局（每行可不同列数），其余参数忽略
# 示例： layout_mission_control 2880 1800 7 --margin 120 40 40 40  --padding 8 --bases 2x4 --force "7:4,3;9:5,4"
# i=0 x=48 y=128 w=684 h=804
# i=1 x=748 y=128 w=684 h=804
# i=2 x=1448 y=128 w=684 h=804
# i=3 x=2148 y=128 w=684 h=804
# cellW=700
# startX=0
# i=4 x=48 y=948 w=917 h=804
# i=5 x=981 y=948 w=917 h=804
# i=6 x=1914 y=948 w=917 h=804
layout_mission_control() {
  local W="$1" H="$2" N="$3"
  shift 3 || true

  # 默认外边距
  local T=20 R=20 B=20 L=20
  # 默认内边距
  local PT=5 PR=5 PB=5 PL=5
  local BASES="" FORCE_MAP=""

  # 解析命名参数
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --margin)
        if [[ $# -ge 5 && $3 != --* && $4 != --* && $5 != --* ]]; then
          # 四个值：--margin T R B L
          T="$2"; R="$3"; B="$4"; L="$5"; shift 5
        elif [[ $# -ge 2 && $2 != --* ]]; then
          # 一个值：--margin VALUE
          T="$2"; R="$2"; B="$2"; L="$2"; shift 2
        else
          echo "错误：--margin 需要1个或4个数值参数" >&2; return 1
        fi
        ;;
      --padding)
        if [[ $# -ge 5 && $3 != --* && $4 != --* && $5 != --* ]]; then
          # 四个值：--padding PT PR PB PL
          PT="$2"; PR="$3"; PB="$4"; PL="$5"; shift 5
        elif [[ $# -ge 2 && $2 != --* ]]; then
          # 一个值：--padding VALUE
          PT="$2"; PR="$2"; PB="$2"; PL="$2"; shift 2
        else
          echo "错误：--padding 需要1个或4个数值参数" >&2; return 1
        fi
        ;;
      --bases) 
        if [[ $# -ge 2 && $2 != --* ]]; then
          BASES="$2"; shift 2
        else
          echo "错误：--bases 需要参数" >&2; return 1
        fi
        ;;
      --force) 
        if [[ $# -ge 2 && $2 != --* ]]; then
          FORCE_MAP="$2"; shift 2
        else
          echo "错误：--force 需要参数" >&2; return 1
        fi
        ;;
      -*) echo "未知参数：$1" >&2; return 1;;
      *) echo "位置参数错误：$1 (应使用 --margin, --padding 等)" >&2; return 1;;
    esac
  done

  if [[ -z "$W" || -z "$H" || -z "$N" || "$N" -le 0 ]]; then
    echo "usage: mission_control_layout WIDTH HEIGHT COUNT [--margin T R B L] [--padding T R B L] [--bases \"2x4,3x5,...\"] [--force \"7:4,3;...\"]" >&2
    return 1
  fi

  local gridW=$(( W - L - R ))
  local gridH=$(( H - T - B ))
  (( gridW>0 && gridH>0 )) || { echo "invalid margins (grid <= 0)" >&2; return 1; }

  # 如果 --force 命中 N，按强制映射布局（各行列数可不同）
  if [[ -n "$FORCE_MAP" ]]; then
    # 查找 N:pattern
    local pattern
    pattern=$(awk -v map="$FORCE_MAP" -v N="$N" '
      BEGIN{
        n=split(map,items,";")
        for(i=1;i<=n;i++){
          gsub(/^ +| +$/,"",items[i])
          if(items[i]=="") continue
          m=split(items[i],kv,":")
          if(m==2){
            gsub(/^ +| +$/,"",kv[1]); gsub(/^ +| +$/,"",kv[2])
            if(kv[1]==N){ print kv[2]; exit }
          }
        }
      }')
    if [[ -n "$pattern" ]]; then
      # pattern: "4,3" => 两行，第一行4列，第二行3列
      # 逐行等高布局，行高 = floor(gridH / 行数)，每行 cellW = floor(gridW / 该行列数)，并水平居中
      local rows
      rows=$(awk -v p="$pattern" 'BEGIN{print split(p,tmp,",")}')
      local cellH
      if [[ "$rows" -le 0 ]]; then
        echo "错误：行数不能为0或负数" >&2; return 1
      fi
      cellH=$(awk -v gh="$gridH" -v r="$rows" 'BEGIN{printf "%d", int(gh/r)}')
      local y="$T"
      local i=0

      # 将逗号分隔的字符串转换为数组，兼容不同shell
      ROWS_ARR=()
      while IFS=',' read -r -d ',' value || [[ -n "$value" ]]; do
        ROWS_ARR+=("$value")
      done <<< "$pattern,"
      for rc in "${ROWS_ARR[@]}"; do
        local cols="$rc"
        # 这一行的 cellW 和水平起始偏移（居中）
        local cellW startX
        if [[ "$cols" -le 0 ]]; then
          echo "错误：列数不能为0或负数" >&2; return 1
        fi
        cellW=$(awk -v gw="$gridW" -v c="$cols" 'BEGIN{printf "%d", int(gw/c)}')
        startX=$(awk -v gw="$gridW" -v c="$cols" -v cw="$cellW" 'BEGIN{printf "%d", int((gw - c*cw)/2)}')
        local x=$(( L + startX ))
        local j=0
        while (( j < cols && i < N )); do
          # 输出时减去内边距
          printf "i=%d x=%d y=%d w=%d h=%d;" \
            "$i" \
            "$((x + PL))" \
            "$((y + PT))" \
            "$((cellW - PL - PR))" \
            "$((cellH - PT - PB))"
          x=$(( x + cellW ))
          j=$(( j + 1 ))
          i=$(( i + 1 ))
        done
        y=$(( y + cellH ))
        (( i >= N )) && return 0
      done
      return 0
    fi
  fi

  # 生成候选列数（含：平方/近似平方 + bases + bases 的邻居）
  # 评分规则：
  #   1) 优先 remainder = cols-1（即末行只少 1 个）
  #   2) 面积 rows*cols 尽量小
  #   3) 行列差 |rows-cols| 尽量小
  # 说明：rows=ceil(N/cols)
  read -r rows cols < <(awk -v N="$N" -v bases="$BASES" '
    function ceil(x){ return (x==int(x)?x:int(x)+1) }
    function abs(x){ return x<0?-x:x }
    function addcol(c){ if(c<1) return; seen[c]++; cols[++m]=c }

    BEGIN{
      # 候选 1：近似平方（以 sqrt 为中心 ±20%）
      root = sqrt(N)
      cmin = int(root*0.8); if(cmin<1) cmin=1
      cmax = ceil(root*1.2)
      for(c=cmin; c<=cmax; c++) addcol(c)

      # 候选 2：用户 bases（及邻居 r-1 x c+1 -> 用其列 c 与 c+1）
      if(length(bases)){
        n=split(bases,arr,",")
        for(i=1;i<=n;i++){
          gsub(/^ +| +$/,"",arr[i]); if(arr[i]=="") continue
          split(arr[i],rc,"x"); r=rc[1]+0; c=rc[2]+0
          if(c>0) addcol(c)
          if(r>1) addcol(c+1)   # 邻居  (r-1) x (c+1)，体现你给的 2x4 -> 1x5 示例
        }
      }

      # 候选 3：兜底（1..min(N, 2*root+5)）
      limit = int(2*root+5); if(limit<1) limit=1; if(limit>N) limit=N
      for(c=1; c<=limit; c++) addcol(c)

      # 选择最优
      bestDef=1e9; bestArea=1e18; bestDiff=1e18; br=0; bc=0
      for(i=1;i<=m;i++){
        c = cols[i]
        rows = ceil(N/c)
        lastCount = N - (rows-1)*c; if(lastCount<=0) lastCount=c
        def = c - lastCount  # 末行缺口
        # 评分 1：def==1 最优 -> 赋 0；否则 1
        pref = (def==1 ? 0 : 1)
        area = rows*c
        diff = abs(rows-c)
        if( pref < bestDef ||
           (pref==bestDef && (area<bestArea ||
           (area==bestArea && diff<bestDiff))) ){
          bestDef=pref; bestArea=area; bestDiff=diff; br=rows; bc=c
        }
      }
      print br, bc
    }')

  # 统一网格尺寸（每行等宽列；末行不足时居中）
  local cellW cellH
  if [[ "$cols" -le 0 || "$rows" -le 0 ]]; then
    echo "错误：计算得到的行数或列数无效 (rows=$rows, cols=$cols)" >&2; return 1
  fi
  cellW=$(awk -v gw="$gridW" -v c="$cols" 'BEGIN{printf "%d", int(gw/c)}')
  cellH=$(awk -v gh="$gridH" -v r="$rows" 'BEGIN{printf "%d", int(gh/r)}')

  local used_rows=$(( (N + cols - 1) / cols ))
  local used_prev=$(( (used_rows - 1) * cols ))
  local last_count=$(( N - used_prev ))
  (( last_count<=0 )) && last_count="$cols"

  local i=0
  while (( i < N )); do
    local r=$(( i / cols ))
    local y=$(( T + r * cellH ))
    local this_cols="$cols"
    if (( r == used_rows - 1 )); then this_cols="$last_count"; fi
    local startX
    startX=$(awk -v gw="$gridW" -v c="$cols" -v tc="$this_cols" -v cw="$cellW" \
      'BEGIN{printf "%d", int(((c - tc)*cw)/2)}')
    local x=$(( L + startX ))
    local c=0
    while (( c < this_cols && i < N )); do
      # 输出时减去内边距
      printf "i=%d x=%d y=%d w=%d h=%d;" \
        "$i" \
        "$((x + PL))" \
        "$((y + PT))" \
        "$((cellW - PL - PR))" \
        "$((cellH - PT - PB))"
      x=$(( x + cellW ))
      c=$(( c + 1 ))
      i=$(( i + 1 ))
    done
  done
}

# manage_window "address" --workspace <WS> --position <XxY> --size <WxH>
# 例：manage_window "0x123456789abc" --workspace 4 --position 120x80 --size 1280x720
# manage_window "address" --workspace <WS> --position <XxY> --size <WxH>
# 执行顺序：set floating -> resize -> movetoworkspace -> move

# manage_window "address" --workspace <WS> --position <XxY> --size <WxH>
# 顺序：setfloating on -> resize -> movetoworkspace -> move（位置按目标显示器的局部坐标解释）
# manage_window "address" --workspace <WS> --position <XxY> --size <WxH>
# 顺序：setfloating(toggle) -> resize -> movetoworkspace -> move（位置按目标显示器局部坐标解释）
manage_window() {
  if [[ -z "$1" ]]; then
    echo 'usage: manage_window "address" [--workspace WS] [--position XxY] [--size WxH]' >&2
    return 1
  fi
  local ADDR="$1"; shift

  local WS="" POS="" SIZE=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --workspace) WS="$2"; shift 2;;
      --position)  POS="$2"; shift 2;;
      --size)      SIZE="$2"; shift 2;;
      *) echo "Unknown arg: $1" >&2; return 1;;
    esac
  done

  # 解析位置
  local X="" Y=""
  if [[ -n "$POS" ]]; then
    if [[ "$POS" =~ ^([0-9]+)x([0-9]+)$ ]]; then
      X="${BASH_REMATCH[1]}"; Y="${BASH_REMATCH[2]}"
    else
      echo "Bad --position format. Use XxY, e.g. 200x80" >&2
      return 1
    fi
  fi

  # 解析尺寸
  local W="" H=""
  if [[ -n "$SIZE" ]]; then
    if [[ "$SIZE" =~ ^([0-9]+)x([0-9]+)$ ]]; then
      W="${BASH_REMATCH[1]}"; H="${BASH_REMATCH[2]}"
    else
      echo "Bad --size format. Use WxH, e.g. 500x500" >&2
      return 1
    fi
  fi

  # 1) 强制浮动（便于后续定位与缩放）
  hy_dispatch setfloating "address:${ADDR}" || true

  # 2) 调整大小（如有）
  if [[ -n "$W" && -n "$H" ]]; then
    hy_dispatch resizewindowpixel "exact ${W} ${H},address:${ADDR}" || true
  fi

  # 3) 移动到目标工作区（如有）
  if [[ -n "$WS" ]]; then
    hy_dispatch movetoworkspace "${WS},address:${ADDR}" || true
  fi

  # 4) 按“当前所在显示器”的局部坐标定位（若换了工作区，会以换完后的显示器为基准）
  if [[ -n "$X" && -n "$Y" ]]; then
    # 重新读取窗口所在显示器
    # 以窗口当前所在工作区对应的显示器为基准，避免用光标显示器偏差
    # 先根据地址查找窗口的当前工作区 id，再反查该工作区对应的显示器 id
    read -r curr_ws < <(hyprctl clients -j | jq -r --arg addr "$ADDR" '.[] | select(.address==$addr) | .workspace.id')
    read -r mid < <(hyprctl workspaces -j | jq -r --argjson wid "$curr_ws" '.[] | select(.id==$wid) | .monitorID // .monitor // .monitorId // .monitor_id')
    read -r mon_x mon_y < <(hyprctl monitors -j | jq -r --argjson id "$mid" '.[] | select(.id==$id) | "\(.x) \(.y)"')

    GX=$(( mon_x + X ))
    GY=$(( mon_y + Y ))

    hy_dispatch movewindowpixel "exact ${GX} ${GY},address:${ADDR}" || true
  fi
}

record_windows_and_move_to_mission_control() {
    local backup_file="/tmp/hycov_windows_backup.txt"
    local special_workspace="special:mission_control"
    
    # Check if currently in special workspace
    local special_workspace_name
    special_workspace_name=$(hyprctl monitors -j | jq -r '.[] | select(.focused) | .specialWorkspace.name')
    dbg "record_windows: special workspace name: ${special_workspace_name}"
    
    # Use different files based on workspace type
    local tmp_file
    if [[ -n "$special_workspace_name" ]]; then
        # In special workspace, use temporary file for current layout only
        tmp_file="/tmp/hycov_current_layout.txt"
        > "$tmp_file"
        dbg "record_windows: using temporary file for special workspace layout: $tmp_file"
    else
        # In normal workspace, use backup file
        tmp_file="$backup_file"
        > "$tmp_file"
        dbg "record_windows: using backup file for normal workspace: $tmp_file"
    fi
    
    # Record windows info based on workspace type
    if [[ -n "$special_workspace_name" ]]; then
        dbg "record_windows: currently in special workspace '${special_workspace_name}', recording only current workspace windows for layout"
        hyprctl clients -j | jq -r --arg curr_ws "$special_workspace_name" '.[] | select(.workspace.name == $curr_ws) | "\(.address)|\(.workspace.id)|\(.at[0])|\(.at[1])|\(.size[0])|\(.size[1])|\(.floating)"' > "$tmp_file"
    else
        dbg "record_windows: in normal workspace, recording all windows for backup"
        hyprctl clients -j | jq -r '.[] | "\(.address)|\(.workspace.id)|\(.at[0])|\(.at[1])|\(.size[0])|\(.size[1])|\(.floating)"' > "$tmp_file"
    fi
    
    # Get cursor position and monitor dimensions
    get_cursor_position
    get_cursor_monitor_wh
    
    # Count windows
    local window_count=$(wc -l < "$tmp_file")
    dbg "record_windows: count=${window_count}"
    
    if [[ "$window_count" -le 0 ]]; then
        echo "No windows found"
        return 0
    fi
    
    # 执行 layout_mission_control 获取布局，每个窗口一行
    local layout_output
    layout_output=$(layout_mission_control "$curr_monitor_width" "$curr_monitor_height" "$window_count" --margin 80 40 40 40 --padding 8 --bases 2x4 --force "2:2;3:2,1;4:2,2;5:3,2;6:3,3;7:4,3;8:4,4;9:5,4")
    [[ "$HYCOV_DEBUG" = "1" ]] && echo "[DEBUG] layout:\n${layout_output}" || true
    
    # Parse layout output and move windows
    local i=0
    while IFS='|' read -r address workspace orig_x orig_y orig_w orig_h orig_float; do
        if [[ -z "$address" ]]; then continue; fi
        dbg "record_windows: i=${i} address=${address} ws=${workspace} orig=${orig_x},${orig_y} ${orig_w}x${orig_h} float=${orig_float}"
        
        # Get position from layout output for window index i
        local layout_line
        layout_line=$(echo "$layout_output" | cut -d';' -f$((i+1)))
        
        if [[ -n "$layout_line" ]]; then
            # Parse layout line: i=0 x=25 y=25 w=640 h=360
            local new_x new_y new_w new_h
            eval $(echo "$layout_line" | sed 's/i=[0-9]* //; s/ /; /g; s/=/=/g')
            dbg "record_windows: apply -> x=${x} y=${y} w=${w} h=${h}"
            # Move window with new position and size
            if [[ -n "$special_workspace_name" ]]; then
                # In special workspace, just reposition without changing workspace
                manage_window "$address" --position "${x}x${y}" --size "${w}x${h}"
            else
                # In normal workspace, move to mission control workspace
                manage_window "$address" --workspace "$special_workspace" --position "${x}x${y}" --size "${w}x${h}"
            fi
        fi
        
        ((i++))
    done < "$tmp_file"
    
    if [[ -n "$special_workspace_name" ]]; then
        echo "Rearranged $window_count windows in current special workspace: $special_workspace_name"
    else
        echo "Moved $window_count windows to mission control layout in workspace: $special_workspace"
    fi
}

restore_windows_from_backup() {
    local tmp_file="/tmp/hycov_windows_backup.txt"
    
    if [[ ! -f "$tmp_file" ]]; then
        echo "No backup file found at $tmp_file"
        return 1
    fi
    
    # 记录当前焦点窗口，恢复完成后重新聚焦
    local focused_window
    focused_window=$(hyprctl activewindow -j | jq -r '.address // empty')
    dbg "restore_windows: saving focused window: ${focused_window}"
    
    local restored_count=0
    
    while IFS='|' read -r address workspace orig_x orig_y orig_w orig_h orig_float; do
        if [[ -z "$address" ]]; then continue; fi
        
        # Check if window still exists
        if hyprctl clients -j | jq -e --arg addr "$address" '.[] | select(.address == $addr)' > /dev/null 2>&1; then
            # 1) 先移回原工作区
            hy_dispatch movetoworkspace "${workspace},address:${address}" || true
            # 2) 强制浮动，恢复尺寸与坐标
            hy_dispatch setfloating "address:${address}" || true
            hy_dispatch resizewindowpixel "exact ${orig_w} ${orig_h},address:${address}" || true
            # 3) 使用全局坐标恢复位置（orig_x/orig_y 为全局坐标）
            hy_dispatch movewindowpixel "exact ${orig_x} ${orig_y},address:${address}" || true
            # 4) 恢复原始悬浮状态
            if [[ "$orig_float" == "true" ]]; then
              hy_dispatch setfloating "address:${address}" || true
            else
              hy_dispatch settiled "address:${address}" || true
            fi
            ((restored_count++))
        fi
    done < "$tmp_file"
    
    # 恢复完成后，重新聚焦到之前焦点的窗口
    if [[ -n "$focused_window" ]]; then
        dbg "restore_windows: restoring focus to: ${focused_window}"
        # 检查窗口是否仍然存在
        if hyprctl clients -j | jq -e --arg addr "$focused_window" '.[] | select(.address == $addr)' > /dev/null 2>&1; then
            hy_dispatch focuswindow "address:${focused_window}" || true
        else
            dbg "restore_windows: focused window ${focused_window} no longer exists"
        fi
    fi
    
    echo "Restored $restored_count windows to their original positions"
    
    # 检查 special workspace 中是否还有剩余窗口
    local remaining_windows
    remaining_windows=$(hyprctl clients -j | jq -r '.[] | select(.workspace.name | startswith("special:")) | .address' | tr '\n' ' ')
    
    if [[ -n "$remaining_windows" ]]; then
        dbg "restore_windows: found remaining windows in special workspace: $remaining_windows"
        
        # 获取 $focused_window 所在的工作区
        local focused_workspace
        if [[ -n "$focused_window" ]]; then
            focused_workspace=$(hyprctl clients -j | jq -r --arg addr "$focused_window" '.[] | select(.address == $addr) | .workspace.id // empty')
        fi
        
        # 检查 $focused_window 是否在剩余窗口中
        local focused_in_remaining=false
        if [[ -n "$focused_window" ]]; then
            for window in $remaining_windows; do
                if [[ "$window" == "$focused_window" ]]; then
                    focused_in_remaining=true
                    break
                fi
            done
        fi
        
        if [[ "$focused_in_remaining" == "true" ]]; then
            # 如果 $focused_window 在剩余窗口中，移动到所有工作区之后的下一个工作区
            local max_workspace_id
            max_workspace_id=$(hyprctl workspaces -j | jq -r '.[] | select(.id != null) | .id' | sort -n | tail -1)
            local next_workspace=$((max_workspace_id + 1))
            dbg "restore_windows: focused window is in remaining, moving to next workspace: $next_workspace"
            
            for window in $remaining_windows; do
                hy_dispatch movetoworkspace "${next_workspace},address:${window}" || true
            done
        elif [[ -n "$focused_workspace" ]]; then
            # 否则移动到 $focused_window 所在的工作区
            dbg "restore_windows: moving remaining windows to focused window workspace: $focused_workspace"
            
            for window in $remaining_windows; do
                hy_dispatch movetoworkspace "${focused_workspace},address:${window}" || true
            done
        else
            # 如果无法确定目标工作区，移动到工作区 1
            dbg "restore_windows: no target workspace found, moving remaining windows to workspace 1"
            
            for window in $remaining_windows; do
                hy_dispatch movetoworkspace "1,address:${window}" || true
            done
        fi
        
        local remaining_count
        remaining_count=$(echo "$remaining_windows" | wc -w)
        echo "Moved $remaining_count remaining windows from special workspace"
    fi
}

# Main function to handle command line arguments
main() {
    case "${1:-}" in
        --ovon)
            dbg "main: executing record_windows_and_move_to_mission_control"
            record_windows_and_move_to_mission_control
            ;;
        --ovoff)
            dbg "main: executing restore_windows_from_backup"
            restore_windows_from_backup
            ;;
        "")
            # No arguments, do nothing
            dbg "main: no arguments provided, doing nothing"
            ;;
        *)
            echo "Usage: $0 [--ovon|--ovoff]" >&2
            echo "  --ovon   Record windows and move to mission control layout" >&2
            echo "  --ovoff  Restore windows from backup" >&2
            echo "  (no args) Do nothing" >&2
            exit 1
            ;;
    esac
}

# Execute main function with all arguments
main "$@"

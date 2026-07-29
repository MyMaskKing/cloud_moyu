// M1-M3: 单标签 WebView + 老板键 + 自动隐藏
// M2:    多标签 + 画中画 PiP + 视频横屏一体化
// M2 修复: capabilities 覆盖到 web-tab-* + opener scope 放开 http/https
// M2 交互: popout 用系统标题栏可拖可关, close 事件切回 inline
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const DEFAULT_BOSS_KEY: &str = "Ctrl+Alt+KeyQ";

/// 全局隐藏态(老板键切换用)
static HIDDEN: AtomicBool = AtomicBool::new(false);
/// 当前已注册的老板键
static CURRENT_BOSS_KEY: Mutex<Option<Shortcut>> = Mutex::new(None);
/// 当前已注册的"取消透明"快捷键(可选,用户没设就是 None)
static CURRENT_TRANSPARENCY_KEY: Mutex<Option<Shortcut>> = Mutex::new(None);
/// 记录所有已开启的 web tab label,用于老板键批量隐藏/恢复
static WEB_TABS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn tab_label(id: &str) -> String {
    format!("web-tab-{id}")
}

fn remember_tab(label: String) {
    if let Ok(mut g) = WEB_TABS.lock() {
        if !g.iter().any(|l| l == &label) {
            g.push(label);
        }
    }
}

fn forget_tab(label: &str) {
    if let Ok(mut g) = WEB_TABS.lock() {
        g.retain(|l| l != label);
    }
}

// ────── M2: 多标签 WebView 协调 ──────

#[tauri::command]
async fn open_web_tab(
    app: AppHandle,
    id: String,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let label = tab_label(&id);

    // 已存在 = 只切位置/尺寸并显示(可能是复用同一 tab 的 inline 刷新)
    if let Some(existing) = app.get_webview_window(&label) {
        existing.set_position(LogicalPosition::new(x, y)).map_err(|e| e.to_string())?;
        existing.set_size(LogicalSize::new(width, height)).map_err(|e| e.to_string())?;
        existing.show().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let main = app.get_webview_window("main").ok_or("main window missing")?;
    let parsed = WebviewUrl::External(url.parse().map_err(|e: url::ParseError| e.to_string())?);

    // 注入脚本:
    //   1. 重写 window.open / _blank → 在当前 tab 内跳转
    //   2. 拦截 HTML5 fullscreen → 转为主窗口的"应用内全屏"(不让子窗占满 OS)
    //   3. 首个 <video> play → 触发"自动横屏"信号(设置开启才响应)
    let init_script_tpl = r#"
        (function() {
          const TAB_ID = "__MUOYU_TAB_ID__";
          const bridge = window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke;
          const invoke = function(cmd, args) {
            try { if (bridge) bridge(cmd, args || {}); } catch (e) {}
          };

          // ── window.open / _blank 拦截 ──
          const nativeOpen = window.open;
          window.open = function(url, target, features) {
            if (url) {
              try {
                window.location.href = new URL(url, window.location.href).href;
              } catch (e) {
                return nativeOpen.call(window, url, target, features);
              }
              return window;
            }
            return nativeOpen.call(window, url, target, features);
          };
          document.addEventListener('click', function(e) {
            const a = e.target && e.target.closest && e.target.closest('a[href]');
            if (a && a.target === '_blank' && a.href) {
              e.preventDefault();
              e.stopPropagation();
              window.location.href = a.href;
            }
          }, true);

          // ── HTML5 fullscreen 拦截:改由主窗口做"应用内全屏" ──
          //   同时把目标元素套 position:fixed 全屏样式,让视频真填满 webview
          let fakeFsEl = null;
          let fakeFsPrev = null; // 保存原始 style,退出时还原
          const FS_STYLE = 'position:fixed !important;left:0 !important;top:0 !important;width:100vw !important;height:100vh !important;z-index:2147483647 !important;background:#000 !important;margin:0 !important;padding:0 !important;';
          function enterFake(el) {
            fakeFsEl = el;
            try {
              fakeFsPrev = el.getAttribute('style') || '';
              el.setAttribute('style', fakeFsPrev + ';' + FS_STYLE);
              // 页面 body 禁止滚动
              document.documentElement.style.overflow = 'hidden';
              document.body.style.overflow = 'hidden';
            } catch (e) {}
            try {
              Object.defineProperty(document, 'fullscreenElement', {
                configurable: true, get: function() { return fakeFsEl; }
              });
              Object.defineProperty(document, 'webkitFullscreenElement', {
                configurable: true, get: function() { return fakeFsEl; }
              });
            } catch (e) {}
            try { document.dispatchEvent(new Event('fullscreenchange')); } catch (e) {}
            try { document.dispatchEvent(new Event('webkitfullscreenchange')); } catch (e) {}
            invoke('signal_video_fullscreen', { id: TAB_ID, entering: true });
          }
          function exitFake() {
            try {
              if (fakeFsEl) {
                if (fakeFsPrev !== null) fakeFsEl.setAttribute('style', fakeFsPrev);
                else fakeFsEl.removeAttribute('style');
              }
              document.documentElement.style.overflow = '';
              document.body.style.overflow = '';
            } catch (e) {}
            fakeFsEl = null;
            fakeFsPrev = null;
            try {
              Object.defineProperty(document, 'fullscreenElement', {
                configurable: true, get: function() { return null; }
              });
              Object.defineProperty(document, 'webkitFullscreenElement', {
                configurable: true, get: function() { return null; }
              });
            } catch (e) {}
            try { document.dispatchEvent(new Event('fullscreenchange')); } catch (e) {}
            try { document.dispatchEvent(new Event('webkitfullscreenchange')); } catch (e) {}
            invoke('signal_video_fullscreen', { id: TAB_ID, entering: false });
          }
          const proto = Element.prototype;
          proto.requestFullscreen = function() { enterFake(this); return Promise.resolve(); };
          if (proto.webkitRequestFullscreen)     proto.webkitRequestFullscreen     = function() { enterFake(this); };
          if (proto.webkitRequestFullScreen)     proto.webkitRequestFullScreen     = function() { enterFake(this); };
          if (proto.msRequestFullscreen)         proto.msRequestFullscreen         = function() { enterFake(this); };
          document.exitFullscreen        = function() { exitFake(); return Promise.resolve(); };
          document.webkitExitFullscreen  = function() { exitFake(); };
          document.msExitFullscreen      = function() { exitFake(); };
          // 键盘 ESC / F 退出:hook keydown
          document.addEventListener('keydown', function(e) {
            if (e.key === 'Escape' && fakeFsEl) { e.stopPropagation(); exitFake(); }
          }, true);

          // ── 视频 play → 自动横屏信号(严格过滤:短视频/封面缩略图不触发) ──
          //   条件:videoWidth ≥ 480,duration > 60s,宽高比 > 1.4(横屏)
          let videoSignaled = false;
          function checkVideoForAutoFs(v) {
            if (videoSignaled) return;
            if (!v || v.tagName !== 'VIDEO') return;
            const w = v.videoWidth || 0;
            const h = v.videoHeight || 0;
            const d = v.duration || 0;
            if (w < 480 || h < 1 || d < 60) return;
            if (w / h < 1.4) return;
            videoSignaled = true;
            invoke('signal_video_play', { id: TAB_ID });
          }
          document.addEventListener('play', function(e) {
            if (!e.target || e.target.tagName !== 'VIDEO') return;
            // 若 metadata 还没加载,等 loadedmetadata 再判定
            if (e.target.readyState < 1) {
              e.target.addEventListener('loadedmetadata', function once() {
                e.target.removeEventListener('loadedmetadata', once);
                checkVideoForAutoFs(e.target);
              });
            } else {
              checkVideoForAutoFs(e.target);
            }
          }, true);
        })();
    "#;
    let init_script = init_script_tpl.replace("__MUOYU_TAB_ID__", &id);

    let win = WebviewWindowBuilder::new(&app, &label, parsed)
        .parent(&main).map_err(|e| e.to_string())?
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .inner_size(width, height)
        .position(x, y)
        .initialization_script(init_script)
        .build()
        .map_err(|e| e.to_string())?;

    // popout 模式下用户按系统 ✕ 时,不销毁 tab 而是通知前端"回到摸鱼窗口"
    let id_for_event = id.clone();
    let app_for_event = app.clone();
    let label_for_event = label.clone();
    win.on_window_event(move |ev| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = ev {
            // 判断当前是否 popout 状态:decorations 为 true = popout
            // 简化:直接询问 label 有没有 decoration —— 无法直接查,退而求其次:
            // 广播 close-requested 事件给主窗口,由主窗口决定"切回 inline"还是"真关"
            api.prevent_close();
            let _ = app_for_event.emit(
                "web-tab-close-requested",
                serde_json::json!({ "id": id_for_event.clone(), "label": label_for_event.clone() }),
            );
        }
    });

    let _ = win.show();
    remember_tab(label);
    Ok(())
}

#[tauri::command]
async fn resize_web_tab(
    app: AppHandle,
    id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&tab_label(&id)) {
        win.set_position(LogicalPosition::new(x, y)).map_err(|e| e.to_string())?;
        win.set_size(LogicalSize::new(width, height)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn close_web_tab(app: AppHandle, id: String) -> Result<(), String> {
    let label = tab_label(&id);
    if let Some(win) = app.get_webview_window(&label) {
        // 用 destroy() 强制销毁,绕过窗口自身的 CloseRequested 拦截
        // (open_web_tab 里挂了 prevent_close 用来处理"用户点系统 ✕",不能拦到后端主动关闭)
        win.destroy().map_err(|e| e.to_string())?;
    }
    forget_tab(&label);
    Ok(())
}

/// 兜底:关掉所有 web-tab 子窗口(用于"最后一个标签关闭后清空幽灵画面")
#[tauri::command]
async fn close_all_web_tabs(app: AppHandle) -> Result<(), String> {
    let labels = WEB_TABS.lock().map(|g| g.clone()).unwrap_or_default();
    for l in labels {
        if let Some(win) = app.get_webview_window(&l) {
            // 同上:必须 destroy 才能真正关掉,close 会被拦截
            let _ = win.destroy();
        }
    }
    if let Ok(mut g) = WEB_TABS.lock() {
        g.clear();
    }
    Ok(())
}

#[tauri::command]
async fn set_web_tab_visible(app: AppHandle, id: String, visible: bool) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&tab_label(&id)) {
        if visible { win.show().map_err(|e| e.to_string())?; }
        else { win.hide().map_err(|e| e.to_string())?; }
    }
    Ok(())
}

/// 前端查:某 tab 的子窗口当前是否存在(冷启动后 tabs store 已恢复但 webview 还没建时会返回 false)
#[tauri::command]
fn web_tab_exists(app: AppHandle, id: String) -> bool {
    app.get_webview_window(&tab_label(&id)).is_some()
}

/// 获取 web-tab 子窗口在屏幕上的物理坐标矩形,用于"鼠标是否在窗口上"判断
#[tauri::command]
fn web_tab_bounds(app: AppHandle, id: String) -> Option<(i32, i32, u32, u32)> {
    let win = app.get_webview_window(&tab_label(&id))?;
    let pos = win.outer_position().ok()?;
    let size = win.outer_size().ok()?;
    Some((pos.x, pos.y, size.width, size.height))
}

/// 全局光标屏幕坐标(物理像素)
#[tauri::command]
fn get_cursor_position(app: AppHandle) -> Result<(f64, f64), String> {
    let p = app.cursor_position().map_err(|e| e.to_string())?;
    Ok((p.x, p.y))
}

// ────── 导航:后退 / 前进 / 刷新 ──────

#[tauri::command]
async fn web_tab_go_back(app: AppHandle, id: String) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&tab_label(&id)) {
        win.eval("window.history.back()").map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn web_tab_go_forward(app: AppHandle, id: String) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&tab_label(&id)) {
        win.eval("window.history.forward()").map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn web_tab_reload(app: AppHandle, id: String) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&tab_label(&id)) {
        win.eval("window.location.reload()").map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 运行时切换 tab 是否可 resize(popout 独立窗口用)
#[tauri::command]
async fn set_web_tab_resizable(app: AppHandle, id: String, resizable: bool) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&tab_label(&id)) {
        win.set_resizable(resizable).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 切 popout 的窗口外观:是否显示系统标题栏、任务栏、置顶
/// popout 需要:decorations=true + skip_taskbar=false → 可拖可关可任务栏切换
/// inline 需要:decorations=false + skip_taskbar=true → 完全无边框贴在主窗口里
#[tauri::command]
async fn set_web_tab_chrome(
    app: AppHandle,
    id: String,
    decorations: bool,
    skip_taskbar: bool,
    always_on_top: bool,
    title: Option<String>,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&tab_label(&id)) {
        win.set_decorations(decorations).map_err(|e| e.to_string())?;
        win.set_skip_taskbar(skip_taskbar).map_err(|e| e.to_string())?;
        win.set_always_on_top(always_on_top).map_err(|e| e.to_string())?;
        if let Some(t) = title {
            win.set_title(&t).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// popout / inline 切换时用:owner=false 断开与主窗的 owner 关系(主窗最小化不再拖走它);
/// owner=true 重新挂回主窗(inline 状态需要一起隐藏跟随主窗)
#[tauri::command]
fn set_web_tab_owner(app: AppHandle, id: String, owner: bool) -> Result<(), String> {
    let win = match app.get_webview_window(&tab_label(&id)) {
        Some(w) => w,
        None => return Ok(()),
    };

    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowLongPtrW, GWLP_HWNDPARENT,
        };

        let raw = win.hwnd().map_err(|e| e.to_string())?;
        let child_hwnd = HWND(raw.0 as *mut _);

        let new_owner: isize = if owner {
            let main = app.get_webview_window("main").ok_or("main window missing")?;
            let mh = main.hwnd().map_err(|e| e.to_string())?;
            mh.0 as isize
        } else {
            0
        };

        unsafe {
            SetWindowLongPtrW(child_hwnd, GWLP_HWNDPARENT, new_owner);
        }
    }

    #[cfg(not(windows))]
    {
        let _ = (win, owner);
    }
    Ok(())
}

// ────── 右键菜单:独立 always_on_top 窗口,能盖住 web-tab ──────

const CTX_MENU_LABEL: &str = "ctx-menu";

/// 弹出独立右键菜单窗口
/// - `data`:URL-encoded JSON 的菜单项数组
/// - 尺寸由前端预算好传入
#[tauri::command]
async fn show_context_menu(
    app: AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    data: String,
) -> Result<(), String> {
    // 已存在的先关掉,避免重复
    if let Some(old) = app.get_webview_window(CTX_MENU_LABEL) {
        let _ = old.close();
    }

    // 用 query string 把菜单数据传给新窗口的前端
    let url = format!("index.html?view=ctxmenu&data={data}");
    let parsed = WebviewUrl::App(url.into());

    let win = WebviewWindowBuilder::new(&app, CTX_MENU_LABEL, parsed)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focused(true)
        .inner_size(width, height)
        .position(x, y)
        .shadow(false)
        .build()
        .map_err(|e| e.to_string())?;

    // 失焦自动关闭(点其它地方消失)
    let win_close = win.clone();
    win.on_window_event(move |ev| {
        if let tauri::WindowEvent::Focused(false) = ev {
            let _ = win_close.close();
        }
    });

    Ok(())
}

/// 前端主动关闭菜单(选中项后调用)
#[tauri::command]
async fn hide_context_menu(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(CTX_MENU_LABEL) {
        let _ = win.close();
    }
    Ok(())
}

/// 一次性隐藏/显示所有 tab(切换非活跃 tab 时,把上一个隐藏)
#[tauri::command]
async fn set_web_tab_visible_only(app: AppHandle, id: String) -> Result<(), String> {
    let target = tab_label(&id);
    let labels = WEB_TABS.lock().map(|g| g.clone()).unwrap_or_default();
    for l in labels {
        if let Some(win) = app.get_webview_window(&l) {
            if l == target { let _ = win.show(); }
            else { let _ = win.hide(); }
        }
    }
    Ok(())
}

/// 让指定 tab 的子窗口获取键鼠焦点(切换后必调,否则 B 站等站点点击无响应)
#[tauri::command]
async fn focus_web_tab(app: AppHandle, id: String) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&tab_label(&id)) {
        win.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ────── 透明度 ──────

#[tauri::command]
async fn set_web_tab_opacity(app: AppHandle, id: String, opacity: f64) -> Result<(), String> {
    let win = match app.get_webview_window(&tab_label(&id)) {
        Some(w) => w,
        None => return Ok(()),
    };

    #[cfg(windows)]
    {
        use windows::Win32::Foundation::{COLORREF, HWND};
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowLongPtrW,
            GWL_EXSTYLE, LWA_ALPHA, WS_EX_LAYERED,
        };

        let raw = win.hwnd().map_err(|e| e.to_string())?;
        let hwnd = HWND(raw.0 as *mut _);
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0) as u8;

        unsafe {
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_LAYERED.0 as isize);
            SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA)
                .map_err(|e| e.to_string())?;
        }
    }

    #[cfg(not(windows))]
    {
        let _ = (win, opacity);
    }

    Ok(())
}

/// 对所有 tab 批量设置透明度
#[tauri::command]
async fn set_all_web_tabs_opacity(app: AppHandle, opacity: f64) -> Result<(), String> {
    let labels = WEB_TABS.lock().map(|g| g.clone()).unwrap_or_default();
    for l in labels {
        if let Some(id) = l.strip_prefix("web-tab-") {
            let _ = set_web_tab_opacity(app.clone(), id.to_string(), opacity).await;
        }
    }
    Ok(())
}

// ────── PiP: 画中画 ──────

/// 进入画中画:切成置顶顶层小窗
#[tauri::command]
async fn enter_pip(
    app: AppHandle,
    id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let label = tab_label(&id);
    let win = app.get_webview_window(&label).ok_or("tab not found")?;

    win.set_always_on_top(true).map_err(|e| e.to_string())?;
    // Tauri v2 目前不允许运行时切父窗口,直接把窗口置顶+定位即可满足 PiP 场景
    win.set_position(LogicalPosition::new(x, y)).map_err(|e| e.to_string())?;
    win.set_size(LogicalSize::new(width, height)).map_err(|e| e.to_string())?;
    win.show().map_err(|e| e.to_string())?;

    let _ = app.emit("tab-mode-changed", (id, "pip"));
    Ok(())
}

/// 退出画中画:取消置顶,交给前端重新调 resize_web_tab 摆回 holder 位置
#[tauri::command]
async fn exit_pip(app: AppHandle, id: String) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&tab_label(&id)) {
        win.set_always_on_top(false).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("tab-mode-changed", (id, "inline"));
    Ok(())
}

// ────── 视频信号:play(自动横屏) / requestFullscreen(应用内全屏) ──────

/// 子 webview 的注入脚本调用:视频首次 play
#[tauri::command]
fn signal_video_play(app: AppHandle, id: String) -> Result<(), String> {
    let _ = app.emit("web-tab-video-play", id);
    Ok(())
}

/// 子 webview 的注入脚本调用:HTML5 fullscreen 进入/退出
#[tauri::command]
fn signal_video_fullscreen(app: AppHandle, id: String, entering: bool) -> Result<(), String> {
    let _ = app.emit("web-tab-video-fullscreen", serde_json::json!({ "id": id, "entering": entering }));
    Ok(())
}

// ────── M3: 老板键 ──────

/// 让指定 web tab 静音 / 恢复。原理:eval 一段 JS,把页面里所有 <video> <audio> 的 muted 置位。
/// - 隐藏进入时:记录每个元素原始 muted,置为 true
/// - 恢复时:如果我们之前记录过它,则回写原始值(用户在 mute 期间自己开静音的场景也保留)
fn apply_web_tab_mute(app: &AppHandle, label: &str, mute: bool) {
    let js = if mute {
        r#"
        (function(){
            try {
                var els = document.querySelectorAll('video, audio');
                for (var i = 0; i < els.length; i++) {
                    var el = els[i];
                    if (el.__moyuOriginalMuted === undefined) {
                        el.__moyuOriginalMuted = !!el.muted;
                    }
                    el.muted = true;
                }
            } catch(e) {}
        })();
        "#
    } else {
        r#"
        (function(){
            try {
                var els = document.querySelectorAll('video, audio');
                for (var i = 0; i < els.length; i++) {
                    var el = els[i];
                    if (el.__moyuOriginalMuted !== undefined) {
                        el.muted = !!el.__moyuOriginalMuted;
                        delete el.__moyuOriginalMuted;
                    }
                }
            } catch(e) {}
        })();
        "#
    };
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.eval(js);
    }
}

fn toggle_hide(app: &AppHandle) -> bool {
    let now_hidden = !HIDDEN.load(Ordering::Relaxed);
    HIDDEN.store(now_hidden, Ordering::Relaxed);

    if let Some(main) = app.get_webview_window("main") {
        if now_hidden { let _ = main.hide(); }
        else { let _ = main.show(); let _ = main.set_focus(); }
    }
    // 遍历所有 web tab:静音同步 + 显示切换
    let labels = WEB_TABS.lock().map(|g| g.clone()).unwrap_or_default();
    for l in labels {
        // 静音必须先 eval,再 hide;否则 hide 后 webview 可能不响应 eval(某些平台)
        apply_web_tab_mute(app, &l, now_hidden);
        if let Some(win) = app.get_webview_window(&l) {
            if now_hidden { let _ = win.hide(); }
            else { let _ = win.show(); }
        }
    }

    let _ = app.emit("boss-key-toggled", now_hidden);
    now_hidden
}

#[tauri::command]
fn is_hidden() -> bool {
    HIDDEN.load(Ordering::Relaxed)
}

#[tauri::command]
fn trigger_boss_key(app: AppHandle) -> bool {
    toggle_hide(&app)
}

#[tauri::command]
fn update_boss_shortcut(app: AppHandle, shortcut: String) -> Result<(), String> {
    let new_sc: Shortcut = shortcut.parse().map_err(|e| format!("invalid shortcut: {e}"))?;

    let gs = app.global_shortcut();
    if let Ok(mut guard) = CURRENT_BOSS_KEY.lock() {
        if let Some(old) = guard.take() {
            let _ = gs.unregister(old);
        }
        gs.register(new_sc.clone()).map_err(|e| e.to_string())?;
        *guard = Some(new_sc);
    }
    Ok(())
}

/// 设置或清除"取消透明"快捷键。shortcut 为空字符串 = 清除。
#[tauri::command]
fn update_transparency_shortcut(app: AppHandle, shortcut: String) -> Result<(), String> {
    let gs = app.global_shortcut();
    // 先卸载旧的
    if let Ok(mut guard) = CURRENT_TRANSPARENCY_KEY.lock() {
        if let Some(old) = guard.take() {
            let _ = gs.unregister(old);
        }
        if shortcut.trim().is_empty() {
            return Ok(());
        }
        let new_sc: Shortcut = shortcut.parse().map_err(|e| format!("invalid shortcut: {e}"))?;
        gs.register(new_sc.clone()).map_err(|e| e.to_string())?;
        *guard = Some(new_sc);
    }
    Ok(())
}

// ────── 入口 ──────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let default_boss: Shortcut = DEFAULT_BOSS_KEY.parse().expect("bad default shortcut");
    let default_boss_for_handler = default_boss.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        let is_boss = CURRENT_BOSS_KEY
                            .lock()
                            .ok()
                            .and_then(|g| g.clone())
                            .map(|s| &s == shortcut)
                            .unwrap_or_else(|| shortcut == &default_boss_for_handler);
                        if is_boss {
                            toggle_hide(app);
                            return;
                        }
                        let is_transp = CURRENT_TRANSPARENCY_KEY
                            .lock()
                            .ok()
                            .and_then(|g| g.clone())
                            .map(|s| &s == shortcut)
                            .unwrap_or(false);
                        if is_transp {
                            let _ = app.emit("transparency-toggle-requested", ());
                        }
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            open_web_tab,
            resize_web_tab,
            close_web_tab,
            close_all_web_tabs,
            set_web_tab_visible,
            web_tab_exists,
            web_tab_bounds,
            get_cursor_position,
            set_web_tab_visible_only,
            focus_web_tab,
            set_web_tab_opacity,
            set_all_web_tabs_opacity,
            set_web_tab_resizable,
            set_web_tab_chrome,
            set_web_tab_owner,
            web_tab_go_back,
            web_tab_go_forward,
            web_tab_reload,
            enter_pip,
            exit_pip,
            signal_video_play,
            signal_video_fullscreen,
            show_context_menu,
            hide_context_menu,
            is_hidden,
            trigger_boss_key,
            update_boss_shortcut,
            update_transparency_shortcut,
        ])
        .setup(move |app| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
            app.global_shortcut().register(default_boss.clone())?;
            if let Ok(mut g) = CURRENT_BOSS_KEY.lock() {
                *g = Some(default_boss);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

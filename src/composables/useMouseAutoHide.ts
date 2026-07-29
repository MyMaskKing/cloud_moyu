import { onBeforeUnmount, onMounted, watch, type Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTabsStore } from "../stores/tabs";

/**
 * 鼠标离开摸鱼窗口&所有 tab 子窗口 N 秒后,主壳 + 子 webview 一起变淡。
 * 鼠标回到任一窗口范围内立即恢复。
 *
 * 关键:子 webview 是独立 OS 窗口,鼠标进入子窗时对主窗 DOM 而言是 mouseleave,
 * 所以必须用"全局光标位置 vs 每个窗口矩形"判断,不能靠 DOM mouseleave/enter。
 *
 * @param enabled 是否启用
 * @param delayMs 离开多久后触发隐藏
 * @param dim     隐藏时的整体透明度(0-1),默认 0.05
 */
export function useMouseAutoHide(
  enabled: Ref<boolean>,
  delayMs: Ref<number>,
  dim = 0.05,
) {
  const tabs = useTabsStore();
  const mainWin = getCurrentWindow();
  let hidden = false;
  let outsideSince: number | null = null;
  let pollTimer: number | undefined;
  const POLL_MS = 200;

  /** 光标是否在主窗或任一 tab 子窗口的矩形内(物理像素) */
  async function cursorInsideApp(): Promise<boolean> {
    let cx: number, cy: number;
    try {
      const [x, y] = await invoke<[number, number]>("get_cursor_position");
      cx = x; cy = y;
    } catch { return true; /* 拿不到就当在里面,避免误隐 */ }

    // 主窗
    try {
      const mp = await mainWin.outerPosition();
      const ms = await mainWin.outerSize();
      if (cx >= mp.x && cx <= mp.x + ms.width && cy >= mp.y && cy <= mp.y + ms.height) return true;
    } catch {}

    // 每个 tab 子窗
    for (const t of tabs.tabs) {
      try {
        const bounds = await invoke<[number, number, number, number] | null>(
          "web_tab_bounds", { id: t.id },
        );
        if (!bounds) continue;
        const [x, y, w, h] = bounds;
        if (cx >= x && cx <= x + w && cy >= y && cy <= y + h) return true;
      } catch {}
    }
    return false;
  }

  async function doHide() {
    if (hidden) return;
    hidden = true;
    document.body.style.setProperty("--shell-alpha", String(dim));
    await invoke("set_all_web_tabs_opacity", { opacity: dim }).catch(() => {});
  }

  async function doShow() {
    if (!hidden) return;
    hidden = false;
    document.body.style.setProperty("--shell-alpha", "1");
    const restore = parseFloat(getComputedStyle(document.body).getPropertyValue("--web-tab-opacity")) || 1;
    await invoke("set_all_web_tabs_opacity", { opacity: restore }).catch(() => {});
  }

  async function tick() {
    if (!enabled.value) return;
    const inside = await cursorInsideApp();
    const now = performance.now();
    if (inside) {
      outsideSince = null;
      if (hidden) await doShow();
    } else {
      if (outsideSince === null) outsideSince = now;
      if (!hidden && now - outsideSince >= delayMs.value) await doHide();
    }
  }

  function startPolling() {
    stopPolling();
    pollTimer = window.setInterval(tick, POLL_MS);
  }
  function stopPolling() {
    if (pollTimer !== undefined) {
      window.clearInterval(pollTimer);
      pollTimer = undefined;
    }
    outsideSince = null;
  }

  onMounted(() => { if (enabled.value) startPolling(); });
  onBeforeUnmount(() => { stopPolling(); });

  watch(enabled, (v) => {
    if (v) startPolling();
    else {
      stopPolling();
      doShow();
    }
  });
}

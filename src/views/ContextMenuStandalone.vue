<script setup lang="ts">
import { onMounted } from "vue";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface Item { key: string; label: string; icon?: string; divider?: boolean; disabled?: boolean; }

// 从 URL query.data 里拿菜单项
function parseItems(): Item[] {
  try {
    const params = new URLSearchParams(window.location.search);
    const raw = params.get("data");
    if (!raw) return [];
    return JSON.parse(decodeURIComponent(raw));
  } catch {
    return [];
  }
}
const items = parseItems();

const win = getCurrentWindow();

async function pick(it: Item) {
  if (it.disabled || it.divider) return;
  await emit("ctx-menu-pick", it.key);
  await win.close();
}

onMounted(() => {
  // 独立菜单窗口不显示滚动条,keydown Esc 自杀
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") win.close();
  });
});
</script>

<template>
  <ul class="menu">
    <template v-for="(it, i) in items" :key="i">
      <li v-if="it.divider" class="divider"></li>
      <li v-else class="item" :class="{ disabled: it.disabled }" @click="pick(it)">
        <span v-if="it.icon" class="ic">{{ it.icon }}</span>
        <span class="lb">{{ it.label }}</span>
      </li>
    </template>
  </ul>
</template>

<style>
:root, html, body {
  margin: 0; padding: 0;
  background: transparent;
  color: #e6edf3;
  font-family: -apple-system, "Segoe UI", "Microsoft YaHei", sans-serif;
  overflow: hidden;
  user-select: none;
  -webkit-user-select: none;
}
.menu {
  margin: 0;
  padding: 4px;
  list-style: none;
  background: rgba(30, 34, 42, 0.98);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 8px;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  height: 100vh;
  box-sizing: border-box;
}
.item {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 28px;
  padding: 0 10px;
  border-radius: 5px;
  color: #e6edf3;
  font-size: 12px;
  cursor: pointer;
}
.item:hover:not(.disabled) {
  background: #1f6feb;
  color: #fff;
}
.item.disabled {
  color: #484f58;
  cursor: not-allowed;
}
.ic {
  font-size: 12px;
  width: 16px;
  text-align: center;
}
.lb { flex: 1; }
.divider {
  height: 1px;
  margin: 4px 6px;
  background: rgba(255, 255, 255, 0.06);
}
</style>

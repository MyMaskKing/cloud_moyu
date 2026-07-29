<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";

const props = defineProps<{
  autoHide: boolean;
  autoHideDelay: number;
  videoAutoLandscape: boolean;
  bossKey: string;
  transparencyKey: string;
  bypassOpacity: boolean;
}>();

const emit = defineEmits<{
  (e: "update:autoHide", v: boolean): void;
  (e: "update:autoHideDelay", v: number): void;
  (e: "update:videoAutoLandscape", v: boolean): void;
  (e: "update:bossKey", v: string): void;
  (e: "update:transparencyKey", v: string): void;
  (e: "close"): void;
}>();

// 快捷键录制:两个快捷键(boss / transparency)复用同一段状态,用 target 标记当前在录哪个
type RecordTarget = "boss" | "transp";
const recording = ref<RecordTarget | null>(null);
const recordedKey = ref("");

/** 把 KeyboardEvent 转成 Tauri 兼容的快捷键字符串
 *  参考 https://tauri.app/plugin/global-shortcut/ 支持的键名 */
function keyToTauriShortcut(e: KeyboardEvent): string | null {
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Super");

  const key = e.key;
  // 忽略纯修饰键
  if (["Control", "Alt", "Shift", "Meta"].includes(key)) return null;

  let mainKey = "";
  if (/^[a-zA-Z]$/.test(key)) mainKey = "Key" + key.toUpperCase();
  else if (/^[0-9]$/.test(key)) mainKey = "Digit" + key;
  else if (key.startsWith("F") && /^F\d{1,2}$/.test(key)) mainKey = key; // F1-F24
  else if (key === " ") mainKey = "Space";
  else if (key === "Escape") mainKey = "Escape";
  else if (key === "Enter") mainKey = "Enter";
  else if (key === "Tab") mainKey = "Tab";
  else if (key === "ArrowUp") mainKey = "ArrowUp";
  else if (key === "ArrowDown") mainKey = "ArrowDown";
  else if (key === "ArrowLeft") mainKey = "ArrowLeft";
  else if (key === "ArrowRight") mainKey = "ArrowRight";
  else return null;

  parts.push(mainKey);
  return parts.length >= 2 ? parts.join("+") : null; // 至少要有一个修饰键
}

function onRecordKeydown(e: KeyboardEvent) {
  if (!recording.value) return;
  e.preventDefault();
  e.stopPropagation();

  if (e.key === "Escape") {
    recording.value = null;
    return;
  }
  const s = keyToTauriShortcut(e);
  if (s) recordedKey.value = s;
}

async function applyShortcut() {
  if (!recordedKey.value || !recording.value) return;
  const target = recording.value;
  try {
    if (target === "boss") {
      await invoke("update_boss_shortcut", { shortcut: recordedKey.value });
      emit("update:bossKey", recordedKey.value);
    } else {
      await invoke("update_transparency_shortcut", { shortcut: recordedKey.value });
      emit("update:transparencyKey", recordedKey.value);
    }
    recording.value = null;
    recordedKey.value = "";
  } catch (e) {
    alert("快捷键注册失败:" + e + "\n可能被其它应用或本应用其它快捷键占用,请换一个组合");
  }
}

function startRecord(target: RecordTarget) {
  recording.value = target;
  recordedKey.value = "";
}

function cancelRecord() {
  recording.value = null;
  recordedKey.value = "";
}

/** 清除"取消透明"快捷键(设置为空) */
async function clearTransparencyKey() {
  try {
    await invoke("update_transparency_shortcut", { shortcut: "" });
    emit("update:transparencyKey", "");
  } catch (e) {
    alert("清除失败:" + e);
  }
}

onMounted(() => {
  window.addEventListener("keydown", onRecordKeydown, true);
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onRecordKeydown, true);
});

// 显示用：把 Tauri 格式转回好看的展示
function humanize(sc: string): string {
  return sc.split("+").map(p => {
    if (p.startsWith("Key")) return p.slice(3);
    if (p.startsWith("Digit")) return p.slice(5);
    return p;
  }).join(" + ");
}
const bossKeyDisplay = computed(() => humanize(props.bossKey));
const transpKeyDisplay = computed(() => props.transparencyKey ? humanize(props.transparencyKey) : "未设置");
</script>

<template>
  <section class="settings-view">
    <header class="settings-head" data-tauri-drag-region>
      <button class="back" @click="emit('close')" title="返回">‹ 返回</button>
      <h2 data-tauri-drag-region>设置</h2>
      <div class="spacer" data-tauri-drag-region></div>
    </header>

    <div class="settings-body">
      <!-- 老板快捷键 -->
      <div class="group">
        <div class="group-title">🚨 老板快捷键</div>
        <div class="group-desc">按下瞬间隐藏整个应用,再按一次恢复。至少需要一个修饰键 (Ctrl/Alt/Shift)。</div>
        <div class="group-content">
          <div class="row">
            <span class="label">当前快捷键</span>
            <kbd v-if="recording !== 'boss'" class="kbd">{{ bossKeyDisplay }}</kbd>
            <kbd v-else class="kbd recording">
              {{ recordedKey ? humanize(recordedKey) : '按下组合键…' }}
            </kbd>
          </div>
          <div class="row actions">
            <button v-if="recording !== 'boss'" class="btn" :disabled="!!recording" @click="startRecord('boss')">✎ 修改</button>
            <template v-else>
              <button class="btn primary" :disabled="!recordedKey" @click="applyShortcut">应用</button>
              <button class="btn" @click="cancelRecord">取消</button>
              <span class="hint">按 Esc 取消</span>
            </template>
          </div>
        </div>
      </div>

      <!-- 取消透明快捷键 -->
      <div class="group">
        <div class="group-title">👁 取消透明快捷键</div>
        <div class="group-desc">
          透明状态下按钮不好点?按下这个组合键立即把窗口和网页恢复到不透明,再按一次回到原透明度。
          期间"鼠标离开自动隐藏"也会临时关闭,方便操作。
        </div>
        <div class="group-content">
          <div class="row">
            <span class="label">当前快捷键</span>
            <kbd v-if="recording !== 'transp'" class="kbd" :class="{ 'kbd-empty': !transparencyKey }">{{ transpKeyDisplay }}</kbd>
            <kbd v-else class="kbd recording">
              {{ recordedKey ? humanize(recordedKey) : '按下组合键…' }}
            </kbd>
            <span v-if="transparencyKey && !recording" class="hint" style="margin-left:8px">
              {{ bypassOpacity ? '当前:已取消透明' : '当前:按设置的透明度显示' }}
            </span>
          </div>
          <div class="row actions">
            <template v-if="recording !== 'transp'">
              <button class="btn" :disabled="!!recording" @click="startRecord('transp')">
                {{ transparencyKey ? '✎ 修改' : '＋ 设置' }}
              </button>
              <button v-if="transparencyKey" class="btn" :disabled="!!recording" @click="clearTransparencyKey">清除</button>
            </template>
            <template v-else>
              <button class="btn primary" :disabled="!recordedKey" @click="applyShortcut">应用</button>
              <button class="btn" @click="cancelRecord">取消</button>
              <span class="hint">按 Esc 取消</span>
            </template>
          </div>
        </div>
      </div>

      <!-- 视频自动横屏 -->
      <div class="group">
        <div class="group-title">🎬 视频播放自动横屏</div>
        <div class="group-desc">检测到当前页面播放视频时,自动切换到 16:9 宽屏比例并进入沉浸模式。当前为占位实现,与画中画一同在后续版本开放。</div>
        <div class="group-content">
          <div class="row">
            <span class="label">启用</span>
            <label class="switch">
              <input
                type="checkbox"
                :checked="videoAutoLandscape"
                @change="emit('update:videoAutoLandscape', ($event.target as HTMLInputElement).checked)"
              />
              <span class="slider"></span>
            </label>
          </div>
          <div v-if="videoAutoLandscape" class="row hint-row">
            <span class="hint">✓ 已开启,功能上线后自动生效,无需再改</span>
          </div>
        </div>
      </div>

      <!-- 鼠标离开自动隐藏 -->
      <div class="group">
        <div class="group-title">🖱️ 鼠标离开自动隐藏</div>
        <div class="group-desc">鼠标移出窗口指定时间后,主窗口淡出,网页临时隐藏;鼠标移回立即恢复。</div>
        <div class="group-content">
          <div class="row">
            <span class="label">启用</span>
            <label class="switch">
              <input
                type="checkbox"
                :checked="autoHide"
                @change="emit('update:autoHide', ($event.target as HTMLInputElement).checked)"
              />
              <span class="slider"></span>
            </label>
          </div>
          <div v-if="autoHide" class="row">
            <span class="label">延迟</span>
            <input
              type="range"
              min="300"
              max="5000"
              step="100"
              :value="autoHideDelay"
              @input="emit('update:autoHideDelay', Number(($event.target as HTMLInputElement).value))"
              style="flex:1;max-width:240px;accent-color:#58a6ff"
            />
            <span class="mono">{{ autoHideDelay }} ms</span>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.settings-view {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  background: rgba(22, 27, 34, calc(var(--shell-alpha) * 0.92));
  z-index: 10;
}

.settings-head {
  height: 44px;
  display: flex;
  align-items: center;
  padding: 0 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  flex-shrink: 0;
}
.settings-head h2 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  color: #e6edf3;
  margin-left: 12px;
}
.spacer { flex: 1; height: 100%; }
.back {
  height: 28px;
  padding: 0 10px;
  border: none;
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.05);
  color: #c9d1d9;
  cursor: pointer;
  font-size: 12px;
}
.back:hover { background: rgba(255, 255, 255, 0.12); }

.settings-body {
  flex: 1;
  overflow-y: auto;
  padding: 20px 24px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.group {
  padding: 16px 18px;
  background: rgba(13, 17, 23, 0.4);
  border: 1px solid rgba(255, 255, 255, 0.05);
  border-radius: 10px;
}
.group-title {
  font-size: 14px;
  font-weight: 600;
  color: #e6edf3;
  margin-bottom: 4px;
}
.group-desc {
  font-size: 12px;
  color: #8b949e;
  margin-bottom: 12px;
  line-height: 1.6;
}
.group-content {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.row.actions { gap: 8px; }
.row.hint-row { margin-top: -4px; }
.label {
  font-size: 12px;
  color: #c9d1d9;
  min-width: 80px;
}
.hint {
  font-size: 11px;
  color: #6e7681;
}
.mono {
  font-family: ui-monospace, "Cascadia Code", Consolas, monospace;
  font-size: 11px;
  color: #58a6ff;
  min-width: 60px;
}
.kbd {
  font-family: ui-monospace, "Cascadia Code", Consolas, monospace;
  font-size: 12px;
  padding: 5px 10px;
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.05);
  color: #58a6ff;
  min-width: 120px;
  text-align: center;
}
.kbd.recording {
  border-color: #f85149;
  color: #f85149;
  animation: pulse 1.2s infinite;
}
.kbd.kbd-empty { color: #6e7681; }
@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.55; }
}

.btn {
  height: 28px;
  padding: 0 12px;
  border: none;
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.06);
  color: #c9d1d9;
  cursor: pointer;
  font-size: 12px;
  transition: background 0.15s;
}
.btn:hover:not(:disabled) { background: rgba(255, 255, 255, 0.12); }
.btn:disabled { opacity: 0.4; cursor: not-allowed; }
.btn.primary { background: #1f6feb; color: #fff; }
.btn.primary:hover:not(:disabled) { background: #388bfd; }

.switch {
  position: relative;
  display: inline-block;
  width: 36px;
  height: 20px;
}
.switch input { opacity: 0; width: 0; height: 0; }
.slider {
  position: absolute;
  inset: 0;
  background: rgba(255, 255, 255, 0.1);
  border-radius: 20px;
  transition: 0.2s;
  cursor: pointer;
}
.slider::before {
  content: "";
  position: absolute;
  left: 3px;
  top: 3px;
  width: 14px;
  height: 14px;
  background: #fff;
  border-radius: 50%;
  transition: 0.2s;
}
.switch input:checked + .slider { background: #1f6feb; }
.switch input:checked + .slider::before { transform: translateX(16px); }
</style>

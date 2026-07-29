<script setup lang="ts">
import { computed } from "vue";

interface MenuItem {
  key: string;
  label: string;
  icon?: string;
  disabled?: boolean;
  divider?: boolean;
}

const props = defineProps<{
  x: number;
  y: number;
  items: MenuItem[];
}>();

const emit = defineEmits<{
  (e: "pick", key: string): void;
  (e: "close"): void;
}>();

// 边界检查:菜单可能溢出视口,右下角需要翻转
const style = computed(() => {
  const w = 180, h = props.items.length * 32 + 8;
  const maxX = window.innerWidth - w - 4;
  const maxY = window.innerHeight - h - 4;
  return {
    left: Math.min(props.x, maxX) + "px",
    top: Math.min(props.y, maxY) + "px",
  };
});

function onPick(item: MenuItem) {
  if (item.disabled || item.divider) return;
  emit("pick", item.key);
}
</script>

<template>
  <div class="menu-mask" @click="emit('close')" @contextmenu.prevent="emit('close')">
    <ul class="menu" :style="style" @click.stop>
      <template v-for="(it, i) in items" :key="i">
        <li v-if="it.divider" class="divider"></li>
        <li v-else class="item" :class="{ disabled: it.disabled }" @click="onPick(it)">
          <span v-if="it.icon" class="ic">{{ it.icon }}</span>
          <span class="lb">{{ it.label }}</span>
        </li>
      </template>
    </ul>
  </div>
</template>

<style scoped>
.menu-mask {
  position: fixed;
  inset: 0;
  z-index: 50;
}
.menu {
  position: absolute;
  min-width: 180px;
  padding: 4px;
  margin: 0;
  list-style: none;
  background: rgba(30, 34, 42, 0.98);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 8px;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  user-select: none;
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

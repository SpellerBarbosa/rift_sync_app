<script setup lang="ts">
// ============================================================
// BaseCard — Cartão base com borda dourada sutil
// Usado em toda a UI para agrupar informações relacionadas
// ============================================================
defineProps<{
  title?: string
  padded?: boolean
}>()
</script>

<template>
  <div
    class="border border-[rgba(200,155,60,0.2)] rounded-xl
           bg-[#1C2A3A]/20 backdrop-blur-sm"
    :class="padded !== false ? 'p-4' : ''"
  >
    <h3
      v-if="title"
      class="text-xs uppercase tracking-widest text-[#C89B3C]/70 mb-3"
    >
      {{ title }}
    </h3>
    <slot />
  </div>
</template>

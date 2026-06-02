<script setup lang="ts">
// ============================================================
// BaseBadge — Badge/tag compacto para categorias e status
// Variantes: gold (destaque), blue (info), red (danger), green
// ============================================================
withDefaults(
  defineProps<{
    variant?: 'gold' | 'blue' | 'red' | 'green'
  }>(),
  { variant: 'gold' }
)
</script>

<template>
  <span
    class="inline-flex items-center text-[10px] font-semibold uppercase tracking-wide
           px-2 py-0.5 rounded"
    :class="{
      'bg-[#C89B3C]/20 text-[#C89B3C]':   variant === 'gold',
      'bg-[#1C2A3A]/60 text-[#E8E0D0]/60': variant === 'blue',
      'bg-[#C0392B]/20 text-[#C0392B]':    variant === 'red',
      'bg-[#27AE60]/20 text-[#27AE60]':    variant === 'green',
    }"
  >
    <slot />
  </span>
</template>

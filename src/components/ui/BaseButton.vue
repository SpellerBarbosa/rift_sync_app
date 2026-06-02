<script setup lang="ts">
// ============================================================
// BaseButton — Botão base do design system RiftSync
// Variantes: primary (dourado), ghost (transparente), danger
// ============================================================
withDefaults(
  defineProps<{
    variant?: 'primary' | 'ghost' | 'danger'
    size?: 'sm' | 'md' | 'lg'
    disabled?: boolean
    loading?: boolean
  }>(),
  {
    variant: 'primary',
    size: 'md',
    disabled: false,
    loading: false,
  }
)
</script>

<template>
  <button
    class="inline-flex items-center justify-center font-medium
           rounded-lg transition-all duration-150 select-none
           focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#C89B3C]/50"
    :class="[
      // Variantes de cor
      variant === 'primary' && 'bg-[#C89B3C] text-[#0A0A0B] hover:bg-[#D4AB4C] active:bg-[#B88B2C]',
      variant === 'ghost'   && 'border border-[rgba(200,155,60,0.3)] text-[#C89B3C] hover:bg-[#C89B3C]/10',
      variant === 'danger'  && 'bg-[#C0392B]/20 border border-[#C0392B]/40 text-[#C0392B] hover:bg-[#C0392B]/30',

      // Tamanhos
      size === 'sm' && 'text-xs px-3 py-1.5 gap-1.5',
      size === 'md' && 'text-sm px-4 py-2 gap-2',
      size === 'lg' && 'text-base px-6 py-3 gap-2.5',

      // Estados
      (disabled || loading) && 'opacity-40 cursor-not-allowed pointer-events-none',
    ]"
    :disabled="disabled || loading"
    v-bind="$attrs"
  >
    <!-- Spinner de loading -->
    <svg
      v-if="loading"
      class="animate-spin -ml-0.5"
      :class="size === 'sm' ? 'h-3 w-3' : 'h-4 w-4'"
      fill="none" viewBox="0 0 24 24"
    >
      <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
      <path class="opacity-75" fill="currentColor"
            d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
    </svg>

    <slot />
  </button>
</template>

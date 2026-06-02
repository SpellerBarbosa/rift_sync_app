<script setup lang="ts">
// ============================================================
// InGame — Fase INGAME
// Flashcards de coaching sobrepostos à partida via FlashcardOverlay.
// ============================================================
import FlashcardOverlay from '../overlay/FlashcardOverlay.vue'
</script>

<template>
  <div class="h-full bg-transparent relative">
    <FlashcardOverlay />

    <!-- Indicador central mínimo para debug -->
    <div class="flex items-center justify-center h-full pointer-events-none">
      <p class="text-rs-white/10 text-xs font-mono">em partida</p>
    </div>
  </div>
</template>

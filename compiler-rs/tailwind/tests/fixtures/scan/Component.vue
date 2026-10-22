<template lang="pug">
div.flex.items-center(:class="{ 'bg-red-500': error, 'text-white': error }")
  span.text-sm.font-medium {{ label }}
</template>

<template>
  <div :class="['p-4', active && 'ring-2 ring-blue-500']" class="rounded-xl shadow-lg">
    <button v-bind:class="{ 'opacity-50 cursor-not-allowed': disabled }" @click="go">Go</button>
    <transition enter-active-class="transition duration-300 ease-out" enter-from-class="opacity-0 scale-95">
      <p v-if="open" class="mt-2 text-gray-600 dark:text-gray-300">Hi</p>
    </transition>
  </div>
</template>

<script setup lang="ts">
const classes = computed(() => (props.size === 'lg' ? 'text-lg px-6' : 'text-sm px-3'))
</script>

<style scoped>
.btn { @apply px-4 py-2; }
</style>

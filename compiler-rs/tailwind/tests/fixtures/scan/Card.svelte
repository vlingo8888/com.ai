<script>
  export let active = false;
  let size = 'md';
</script>

<div class="card flex flex-col gap-2" class:bg-blue-500={active} class:text-white={active}>
  <h2 class:font-bold class:underline={hovered}>Title</h2>
  <p class="text-{size} leading-relaxed">Body</p>
	<span class:opacity-50={!active}>x</span>
  {#if active}
    <button class="btn px-4 py-2 hover:bg-blue-600 focus:outline-none">Ok</button>
  {/if}
</div>

<style>
  .card { border-radius: 0.5rem; }
</style>

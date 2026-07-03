<script lang="ts">
  import type { AgentConfig } from "./types";
  import SpawnForm from "./SpawnForm.svelte";
  import Modal from "./ui/Modal.svelte";

  let {
    onspawn,
    oncancel,
  }: {
    onspawn: (config: AgentConfig) => void;
    oncancel: () => void;
  } = $props();
</script>

<Modal title="Create agent" onclose={oncancel} width={560}>
  <SpawnForm {onspawn} {oncancel} />
</Modal>

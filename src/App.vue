<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { api, type InstanceCfg, type UUID } from "./api";
import InstancesPanel from "./components/InstancesPanel.vue";
import KeyBrowser from "./components/KeyBrowser.vue";
import PubSubPanel from "./components/PubSubPanel.vue";

const instances = ref<InstanceCfg[]>([]);
const selectedId = ref<UUID | null>(null);
const connected = ref(false);

async function reload() {
    instances.value = await api.listInstances();
    if (!selectedId.value && instances.value.length) {
        selectedId.value = instances.value[0].id;
    }
}
async function onSave(cfg: InstanceCfg) {
    await api.saveInstance(cfg);
    await reload();
}
async function onRemove(id: UUID) {
    await api.removeInstance(id);
    if (selectedId.value === id) selectedId.value = null;
    await reload();
}
async function toggleConnect(id: UUID) {
    if (!connected.value) {
        await api.connect(id);
        connected.value = true;
    } else {
        await api.disconnect(id);
        connected.value = false;
    }
}

const current = computed(
    () => instances.value.find((i) => i.id === selectedId.value) ?? null,
);

onMounted(reload);
</script>

<template>
    <div class="app">
        <aside class="sidebar">
            <InstancesPanel
                :items="instances"
                :selectedId="selectedId"
                @select="(id) => (selectedId = id)"
                @save="onSave"
                @remove="onRemove"
                @connect="toggleConnect"
                :connected="connected"
            />
        </aside>

        <main class="content" v-if="current">
            <KeyBrowser :instanceId="current.id" :connected="connected" />
            <div style="height: 16px" />
            <PubSubPanel :instanceId="current.id" :connected="connected" />
        </main>

        <main class="content" v-else>
            <h3>No instance selected</h3>
        </main>
    </div>
</template>

<style scoped>
.app {
    display: grid;
    grid-template-columns: 340px 1fr;
    height: 100vh;
}
.sidebar {
    border-right: 1px solid #333;
    padding: 12px;
    overflow: auto;
}
.content {
    padding: 16px;
    overflow: auto;
}
</style>

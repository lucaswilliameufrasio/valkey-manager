<script setup lang="ts">
import { ref, watch, onMounted } from "vue";
import { api, type UUID } from "../api";
import KeyInspector from "./KeyInspector.vue";

const props = defineProps<{ instanceId: UUID; connected: boolean }>();

const pattern = ref("*");
const cursor = ref(0);
const count = ref(100);
const keys = ref<string[]>([]);
const selectedKey = ref<string | null>(null);

async function load() {
    if (!props.connected) return;
    const [next, list] = await api.scan(
        props.instanceId,
        pattern.value,
        cursor.value,
        count.value,
    );
    cursor.value = next;
    keys.value = list;
    if (selectedKey.value && !keys.value.includes(selectedKey.value))
        selectedKey.value = null;
}
function prevPage() {
    /* SCAN is forward only: reset + increase COUNT as a workaround or keep a small cache.
                         Here we just reset to 0 to start over. */
    cursor.value = 0;
    load();
}
function nextPage() {
    load();
}

watch([() => props.instanceId, () => props.connected], () => {
    cursor.value = 0;
    load();
});
watch([pattern, count], () => {
    cursor.value = 0;
    load();
});

onMounted(load);
</script>

<template>
    <section>
        <h3>Key Browser</h3>
        <div style="display: flex; gap: 8px; align-items: center">
            <input
                v-model="pattern"
                placeholder="Pattern (e.g. user:*, *)"
                style="flex: 1"
            />
            <input v-model.number="count" type="number" style="width: 120px" />
            <button @click="prevPage">Refresh</button>
            <button @click="nextPage">Next</button>
        </div>

        <div
            style="
                display: grid;
                grid-template-columns: 1fr 1fr;
                gap: 12px;
                margin-top: 12px;
            "
        >
            <div
                style="
                    border: 1px solid #444;
                    padding: 8px;
                    max-height: 50vh;
                    overflow: auto;
                "
            >
                <div
                    v-for="k in keys"
                    :key="k"
                    @click="selectedKey = k"
                    :style="{
                        padding: '6px',
                        cursor: 'pointer',
                        background: selectedKey === k ? '#222' : 'transparent',
                    }"
                >
                    {{ k }}
                </div>
            </div>

            <div
                style="
                    border: 1px solid #444;
                    padding: 8px;
                    max-height: 50vh;
                    overflow: auto;
                "
            >
                <KeyInspector
                    v-if="selectedKey"
                    :instanceId="props.instanceId"
                    :keyName="selectedKey"
                />
                <div v-else>Select a key to inspect</div>
            </div>
        </div>
    </section>
</template>

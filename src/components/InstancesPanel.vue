<script setup lang="ts">
import { ref } from "vue";
import { api, type InstanceCfg, type UUID, type Topology } from "../api";

defineProps<{
    items: InstanceCfg[];
    selectedId: UUID | null;
    connected: boolean;
}>();
const emit = defineEmits<{
    (e: "select", id: UUID): void;
    (e: "save", cfg: InstanceCfg): void;
    (e: "remove", id: UUID): void;
    (e: "connect", id: UUID): void;
}>();

const topoList: Topology[] = ["STANDALONE", "CLUSTER", "SENTINEL"];

const draft = ref<InstanceCfg | null>(null);

function newInstance() {
    api.newInstance().then((cfg) => (draft.value = cfg));
}
function editInstance(i: InstanceCfg) {
    draft.value = JSON.parse(JSON.stringify(i));
}
function applySave() {
    if (draft.value) emit("save", draft.value);
    draft.value = null;
}
</script>

<template>
    <div>
        <div style="display: flex; gap: 8px; margin-bottom: 8px">
            <button @click="newInstance">Add</button>
            <button
                :disabled="!selectedId"
                @click="$emit('remove', selectedId!)"
            >
                Remove
            </button>
        </div>

        <div
            v-for="i in items"
            :key="i.id"
            :style="{
                padding: '8px',
                cursor: 'pointer',
                background: selectedId === i.id ? '#222' : 'transparent',
            }"
            @click="$emit('select', i.id)"
        >
            <div
                style="
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                "
            >
                <div>
                    <strong>{{ i.name }}</strong
                    ><br />
                    <small>{{ i.topology }}</small>
                </div>
                <div style="display: flex; gap: 6px">
                    <button @click.stop="editInstance(i)">Edit</button>
                    <button @click.stop="$emit('connect', i.id)">
                        {{ connected ? "Disconnect" : "Connect" }}
                    </button>
                </div>
            </div>
        </div>

        <div
            v-if="draft"
            style="
                margin-top: 12px;
                border-top: 1px solid #444;
                padding-top: 12px;
            "
        >
            <h4>Edit Instance</h4>
            <div style="display: grid; gap: 8px">
                <label>Name <input v-model="draft.name" /></label>
                <label
                    >Topology
                    <select v-model="draft.topology">
                        <option v-for="t in topoList" :key="t" :value="t">
                            {{ t }}
                        </option>
                    </select>
                </label>
                <label
                    ><input type="checkbox" v-model="draft.use_tls" />
                    TLS</label
                >
                <div style="display: flex; gap: 8px">
                    <label>Username <input v-model="draft.username" /></label>
                    <label
                        >Password
                        <input v-model="draft.password" type="password"
                    /></label>
                    <label
                        >DB
                        <input
                            v-model.number="draft.db"
                            type="number"
                            style="width: 80px"
                    /></label>
                </div>

                <div
                    v-if="draft.topology === 'STANDALONE'"
                    style="display: flex; gap: 8px"
                >
                    <label>Host <input v-model="draft.host" /></label>
                    <label
                        >Port
                        <input
                            v-model.number="draft.port"
                            type="number"
                            style="width: 120px"
                    /></label>
                </div>

                <div v-if="draft.topology === 'CLUSTER'">
                    <label
                        >Cluster nodes (comma separated host:port)
                        <input
                            :value="draft.cluster_nodes.join(',')"
                            @input="
                                draft.cluster_nodes = (
                                    $event.target as HTMLInputElement
                                ).value
                                    .split(',')
                                    .map((s) => s.trim())
                                    .filter(Boolean)
                            "
                        />
                    </label>
                </div>

                <div v-if="draft.topology === 'SENTINEL'">
                    <label
                        >Sentinel masterId
                        <input v-model="draft.sentinel_master_id"
                    /></label>
                    <label
                        >Sentinel nodes (comma separated host:port)
                        <input
                            :value="draft.sentinel_nodes.join(',')"
                            @input="
                                draft.sentinel_nodes = (
                                    $event.target as HTMLInputElement
                                ).value
                                    .split(',')
                                    .map((s) => s.trim())
                                    .filter(Boolean)
                            "
                        />
                    </label>
                </div>

                <div style="display: flex; gap: 8px; margin-top: 8px">
                    <button @click="applySave">Save</button>
                    <button @click="draft = null">Cancel</button>
                </div>
            </div>
        </div>
    </div>
</template>

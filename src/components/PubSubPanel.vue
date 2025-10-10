<script setup lang="ts">
import { ref, onMounted } from "vue";
import { api, type UUID } from "../api";

const props = defineProps<{ instanceId: UUID; connected: boolean }>();

const channel = ref("");
const pattern = ref("");
const pubChannel = ref("");
const message = ref("");

const logs = ref<string[]>([]);
const subs = ref<string[]>([]); // sub ids

function log(s: string) {
    logs.value = [s, ...logs.value].slice(0, 500);
}

async function sub() {
    if (!channel.value) return;
    const id = await api.subscribe(props.instanceId, channel.value);
    subs.value.push(id);
    log(`SUBSCRIBE '${channel.value}'`);
    channel.value = "";
}
async function psub() {
    if (!pattern.value) return;
    const id = await api.psubscribe(props.instanceId, pattern.value);
    subs.value.push(id);
    log(`PSUBSCRIBE '${pattern.value}'`);
    pattern.value = "";
}
async function unsubAll() {
    for (const id of subs.value) {
        await api.unsubscribe(props.instanceId, id);
    }
    subs.value = [];
    log("Unsubscribed all");
}
async function publish() {
    if (!pubChannel.value) return;
    const n = await api.publish(
        props.instanceId,
        pubChannel.value,
        message.value,
    );
    log(`PUBLISH to '${pubChannel.value}' (${n} receivers): ${message.value}`);
}

onMounted(() => {
    api.onPubSubMessage(({ channel, message }) =>
        log(`[${channel}] ${message}`),
    );
});
</script>

<template>
    <section>
        <h3>Pub/Sub</h3>
        <div style="display: flex; gap: 8px">
            <input v-model="channel" placeholder="Channel" />
            <button @click="sub">SUBSCRIBE</button>
            <input v-model="pattern" placeholder="Pattern" />
            <button @click="psub">PSUBSCRIBE</button>
            <button @click="unsubAll">UNSUB ALL</button>
        </div>

        <div style="display: flex; gap: 8px; margin-top: 8px">
            <input
                v-model="pubChannel"
                placeholder="Publish channel"
                style="flex: 1"
            />
            <input v-model="message" placeholder="Message" style="flex: 1" />
            <button @click="publish">PUBLISH</button>
        </div>

        <div
            style="
                margin-top: 8px;
                border: 1px solid #444;
                height: 220px;
                overflow: auto;
                padding: 8px;
            "
        >
            <div v-for="(l, i) in logs" :key="i">{{ l }}</div>
        </div>
    </section>
</template>

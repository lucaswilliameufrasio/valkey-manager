<script setup lang="ts">
import { ref, onMounted, watch, defineComponent, computed } from "vue";
import { api, type UUID } from "../api";

const props = defineProps<{ instanceId: UUID; keyName: string }>();

const typeStr = ref<string>("...");
const ttl = ref<number | null>(null);
const renameTo = ref(props.keyName);
const ttlInput = ref<string>("");

async function loadMeta() {
    typeStr.value = await api.keyType(props.instanceId, props.keyName);
    const t = await api.getTTL(props.instanceId, props.keyName);
    ttl.value = t >= 0 ? t : null;
    renameTo.value = props.keyName;
    ttlInput.value = ttl.value != null ? String(ttl.value) : "";
}
onMounted(loadMeta);
watch(() => props.keyName, loadMeta);

async function applyRename() {
    await api.renameKey(props.instanceId, props.keyName, renameTo.value);
    await loadMeta();
}
async function applyTTL() {
    await api.setTTL(
        props.instanceId,
        props.keyName,
        ttlInput.value.trim() ? Number(ttlInput.value) : null,
    );
    await loadMeta();
}
async function del() {
    await api.delKey(props.instanceId, props.keyName);
}

const StringEditor = defineComponent({
    name: "StringEditor",
    props: {
        instanceId: { type: String, required: true },
        keyName: { type: String, required: true },
    },
    setup(props: { instanceId: UUID; keyName: string }) {
        const cur = ref<string | null>(null);
        const newVal = ref("");
        const ttl = ref("");

        const currentValue = computed(() => cur.value ?? "∅");

        const load = async () => {
            cur.value = await api.getString(props.instanceId, props.keyName);
            newVal.value = cur.value ?? "";
        };

        onMounted(load);

        const apply = async () => {
            await api.setString(
                props.instanceId,
                props.keyName,
                newVal.value,
                ttl.value.trim() ? Number(ttl.value) : null,
            );
            await load();
        };

        return {
            newVal,
            ttl,
            apply,
            currentValue,
        };
    },
    template: `
        <div>
            <div>
                <strong>Current:</strong> {{ currentValue }}
            </div>
            <textarea
                v-model="newVal"
                style="width:100%;height:120px;"
            ></textarea>
            <div style="display:flex; gap:8px; align-items:center;">
                <input
                    v-model="ttl"
                    placeholder="TTL (s, optional)"
                    style="width:180px;"
                />
                <button @click="apply">SET</button>
            </div>
        </div>
    `,
});

const ListEditor = defineComponent({
    name: "ListEditor",
    props: {
        instanceId: { type: String, required: true },
        keyName: { type: String, required: true },
    },
    setup(props: { instanceId: UUID; keyName: string }) {
        const start = ref(0);
        const win = ref<string[]>([]);
        const pushText = ref("");

        const load = async () => {
            win.value = await api.listRange(
                props.instanceId,
                props.keyName,
                start.value,
                start.value + 49,
            );
        };

        onMounted(load);

        const prev = async () => {
            start.value = Math.max(0, start.value - 50);
            await load();
        };

        const next = async () => {
            start.value += 50;
            await load();
        };

        const parseValues = () =>
            pushText.value
                .split(",")
                .map((s) => s.trim())
                .filter(Boolean);

        const lpush = async () => {
            const vals = parseValues();
            if (vals.length === 0) return;
            await api.listPush(props.instanceId, props.keyName, true, vals);
            pushText.value = "";
            await load();
        };

        const rpush = async () => {
            const vals = parseValues();
            if (vals.length === 0) return;
            await api.listPush(props.instanceId, props.keyName, false, vals);
            pushText.value = "";
            await load();
        };

        return {
            start,
            win,
            pushText,
            prev,
            next,
            lpush,
            rpush,
        };
    },
    template: `
        <div>
            <div style="display:flex; gap:8px;">
                <button @click="prev">Prev 50</button>
                <button @click="next">Next 50</button>
            </div>
            <div
                v-for="(v, i) in win"
                :key="start + '-' + i"
            >
                {{ start + i }}: {{ v }}
            </div>
            <input
                v-model="pushText"
                placeholder="Comma-separated values"
                style="width:100%"
            />
            <div style="display:flex; gap:8px;">
                <button @click="lpush">LPUSH</button>
                <button @click="rpush">RPUSH</button>
            </div>
        </div>
    `,
});

const SetEditor = defineComponent({
    name: "SetEditor",
    props: {
        instanceId: { type: String, required: true },
        keyName: { type: String, required: true },
    },
    setup(props: { instanceId: UUID; keyName: string }) {
        const data = ref<string[]>([]);
        const edit = ref("");

        const load = async () => {
            data.value = await api.setMembers(
                props.instanceId,
                props.keyName,
                200,
            );
        };

        onMounted(load);

        const parseValues = () =>
            edit.value
                .split(",")
                .map((s) => s.trim())
                .filter(Boolean);

        const add = async () => {
            const vals = parseValues();
            if (vals.length === 0) return;
            await api.sadd(props.instanceId, props.keyName, vals);
            edit.value = "";
            await load();
        };

        const rem = async () => {
            const vals = parseValues();
            if (vals.length === 0) return;
            await api.srem(props.instanceId, props.keyName, vals);
            edit.value = "";
            await load();
        };

        return {
            data,
            edit,
            add,
            rem,
        };
    },
    template: `
        <div>
            <div v-for="v in data" :key="v">{{ v }}</div>
            <input
                v-model="edit"
                placeholder="Comma-separated members"
                style="width:100%"
            />
            <div style="display:flex; gap:8px;">
                <button @click="add">SADD</button>
                <button @click="rem">SREM</button>
            </div>
        </div>
    `,
});

const ZSetEditor = defineComponent({
    name: "ZSetEditor",
    props: {
        instanceId: { type: String, required: true },
        keyName: { type: String, required: true },
    },
    setup(props: { instanceId: UUID; keyName: string }) {
        const start = ref(0);
        const win = ref<Array<[string, number]>>([]);
        const member = ref("");
        const score = ref("");

        const load = async () => {
            win.value = await api.zrangeWithscores(
                props.instanceId,
                props.keyName,
                start.value,
                start.value + 49,
            );
        };

        onMounted(load);

        const prev = async () => {
            start.value = Math.max(0, start.value - 50);
            await load();
        };

        const next = async () => {
            start.value += 50;
            await load();
        };

        const parseScore = () => {
            const s = Number(score.value);
            return Number.isNaN(s) ? null : s;
        };

        const add = async () => {
            const s = parseScore();
            if (s == null || !member.value.trim()) return;
            await api.zadd(props.instanceId, props.keyName, member.value, s);
            member.value = "";
            score.value = "";
            await load();
        };

        const rem = async () => {
            if (!member.value.trim()) return;
            await api.zrem(props.instanceId, props.keyName, member.value);
            member.value = "";
            await load();
        };

        const incr = async () => {
            const s = parseScore();
            if (s == null || !member.value.trim()) return;
            await api.zincrby(props.instanceId, props.keyName, member.value, s);
            score.value = "";
            await load();
        };

        return {
            start,
            win,
            member,
            score,
            prev,
            next,
            add,
            rem,
            incr,
        };
    },
    template: `
        <div>
            <div style="display:flex; gap:8px;">
                <button @click="prev">Prev 50</button>
                <button @click="next">Next 50</button>
            </div>
            <div
                v-for="(entry, index) in win"
                :key="entry[0] + '-' + (start + index)"
            >
                {{ start + index }}: [{{ entry[1] }}] {{ entry[0] }}
            </div>
            <div style="display:flex; gap:8px; align-items:center;">
                <input
                    v-model="member"
                    placeholder="Member"
                    style="flex:1"
                />
                <input
                    v-model="score"
                    placeholder="Score"
                    style="width:140px"
                />
            </div>
            <div style="display:flex; gap:8px;">
                <button @click="add">ZADD</button>
                <button @click="rem">ZREM</button>
                <button @click="incr">ZINCRBY</button>
            </div>
        </div>
    `,
});

const HashEditor = defineComponent({
    name: "HashEditor",
    props: {
        instanceId: { type: String, required: true },
        keyName: { type: String, required: true },
    },
    setup(props: { instanceId: UUID; keyName: string }) {
        const data = ref<Array<[string, string]>>([]);
        const field = ref("");
        const value = ref("");

        const load = async () => {
            data.value = await api.hgetall(props.instanceId, props.keyName);
        };

        onMounted(load);

        const hset = async () => {
            if (!field.value.trim()) return;
            await api.hset(
                props.instanceId,
                props.keyName,
                field.value,
                value.value,
            );
            field.value = "";
            value.value = "";
            await load();
        };

        const hdel = async () => {
            if (!field.value.trim()) return;
            await api.hdel(props.instanceId, props.keyName, field.value);
            field.value = "";
            await load();
        };

        return {
            data,
            field,
            value,
            hset,
            hdel,
        };
    },
    template: `
        <div>
            <div
                v-for="entry in data"
                :key="entry[0]"
            >
                {{ entry[0] }} = {{ entry[1] }}
            </div>
            <div style="display:flex; gap:8px;">
                <input
                    v-model="field"
                    placeholder="Field"
                    style="flex:1"
                />
                <input
                    v-model="value"
                    placeholder="Value"
                    style="flex:1"
                />
            </div>
            <div style="display:flex; gap:8px;">
                <button @click="hset">HSET</button>
                <button @click="hdel">HDEL</button>
            </div>
        </div>
    `,
});

const StreamQuickAdd = defineComponent({
    name: "StreamQuickAdd",
    props: {
        instanceId: { type: String, required: true },
        keyName: { type: String, required: true },
    },
    setup(props: { instanceId: UUID; keyName: string }) {
        const text = ref("field=value,foo=bar");

        const add = async () => {
            const pairs = text.value
                .split(",")
                .map((s) => s.trim())
                .filter(Boolean)
                .map((s) => {
                    const idx = s.indexOf("=");
                    if (idx <= 0) return null;
                    return [s.slice(0, idx), s.slice(idx + 1)];
                })
                .filter((entry): entry is [string, string] => Array.isArray(entry));

            if (pairs.length === 0) return;

            await api.xadd(props.instanceId, props.keyName, pairs);
        };

        return {
            text,
            add,
        };
    },
    template: `
        <div>
            <div>
                <strong>Quick XADD</strong>
            </div>
            <input
                v-model="text"
                placeholder="field=value pairs, comma separated"
                style="width:100%"
            />
            <button @click="add">XADD</button>
        </div>
    `,
});
</script>

<template>
    <div>
        <div><strong>Key:</strong> {{ props.keyName }}</div>
        <div>
            <strong>Type:</strong> {{ typeStr }} &nbsp;&nbsp;
            <strong>TTL:</strong> {{ ttl != null ? ttl + "s" : "no TTL" }}
        </div>

        <div style="display: flex; gap: 8px; margin: 8px 0">
            <input v-model="renameTo" placeholder="Rename to" />
            <button @click="applyRename">Rename</button>
            <input
                v-model="ttlInput"
                placeholder="TTL (s), blank = persist"
                style="width: 180px"
            />
            <button @click="applyTTL">Apply TTL</button>
            <button style="background: #b33; color: white" @click="del">
                Delete
            </button>
        </div>

        <div v-if="typeStr === 'string'">
            <StringEditor
                :instanceId="props.instanceId"
                :keyName="props.keyName"
            />
        </div>
        <div v-else-if="typeStr === 'list'">
            <ListEditor
                :instanceId="props.instanceId"
                :keyName="props.keyName"
            />
        </div>
        <div v-else-if="typeStr === 'set'">
            <SetEditor
                :instanceId="props.instanceId"
                :keyName="props.keyName"
            />
        </div>
        <div v-else-if="typeStr === 'zset'">
            <ZSetEditor
                :instanceId="props.instanceId"
                :keyName="props.keyName"
            />
        </div>
        <div v-else-if="typeStr === 'hash'">
            <HashEditor
                :instanceId="props.instanceId"
                :keyName="props.keyName"
            />
        </div>
        <div v-else-if="typeStr === 'stream'">
            <StreamQuickAdd
                :instanceId="props.instanceId"
                :keyName="props.keyName"
            />
        </div>
        <div v-else>Unsupported or unknown type</div>
    </div>
</template>



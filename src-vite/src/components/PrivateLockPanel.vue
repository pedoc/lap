<template>
  <div class="flex flex-col gap-2 text-sm">
    <div class="flex items-center gap-2">
      <h3 class="font-semibold flex-1">{{ $t('private_lock.title') }}</h3>
      <button
        v-if="enabled && !locked && ready"
        class="btn btn-sm"
        :disabled="busy"
        @click="$emit('lock')"
      >
        {{ $t('private_lock.lock_now') }}
      </button>
      <button
        class="btn btn-sm"
        :disabled="busy || !ready"
        @click="settingsOpen = true"
      >
        {{ $t('private_lock.settings') }}
      </button>
    </div>
    <p v-if="!ready" class="text-xs">
      {{ $t('private_lock.checking') }}
      <button class="link" @click="$emit('retry')">
        {{ $t('media_lifecycle.retry') }}
      </button>
    </p>
    <p
      v-if="error"
      role="alert"
      class="text-error whitespace-pre-wrap break-all"
    >
      {{ error }}
    </p>
    <template v-if="ready && enabled && locked">
      <p>{{ $t('private_lock.locked') }}</p>
      <form class="flex gap-2 items-center" @submit.prevent="submitUnlock">
        <input
          v-model="password"
          type="password"
          autocomplete="current-password"
          maxlength="512"
          class="input input-sm"
          :aria-label="$t('private_lock.password')"
          :placeholder="$t('private_lock.password')"
          :disabled="busy"
        />
        <button
          type="submit"
          class="btn btn-primary btn-sm"
          :disabled="busy || !password || retryAfter > 0"
        >
          {{ $t('private_lock.unlock') }}
        </button>
      </form>
      <p v-if="retryAfter" class="text-warning">
        {{ $t('private_lock.cooldown', { seconds: retryAfter }) }}
      </p>
    </template>
    <p class="text-xs text-base-content/60">
      {{ $t('private_lock.not_encryption') }}
    </p>
    <ModalDialog
      v-if="settingsOpen && ready"
      :title="$t('private_lock.settings')"
      :width="480"
      @cancel="clearSettings"
    >
      <form class="flex flex-col gap-3" @submit.prevent="submitSettings">
        <label class="flex gap-2 items-center"
          ><input
            type="checkbox"
            class="checkbox checkbox-sm"
            v-model="desiredEnabled"
            :disabled="busy"
          />{{ $t('private_lock.enable') }}</label
        >
        <label v-if="enabled" class="text-xs"
          >{{ $t('private_lock.current_password')
          }}<input
            v-model="currentPassword"
            type="password"
            autocomplete="current-password"
            maxlength="512"
            class="input input-sm w-full"
            :disabled="busy"
            required
        /></label>
        <template v-if="desiredEnabled">
          <label class="text-xs"
            >{{
              $t(
                enabled
                  ? 'private_lock.new_optional'
                  : 'private_lock.new_password',
              )
            }}<input
              v-model="newPassword"
              type="password"
              autocomplete="new-password"
              maxlength="512"
              class="input input-sm w-full"
              :disabled="busy"
              :required="!enabled"
          /></label>
          <label class="text-xs"
            >{{ $t('private_lock.confirm_password')
            }}<input
              v-model="confirmPassword"
              type="password"
              autocomplete="new-password"
              maxlength="512"
              class="input input-sm w-full"
              :disabled="busy"
              :required="!!newPassword"
          /></label>
          <label class="text-xs"
            >{{ $t('private_lock.idle_minutes')
            }}<input
              v-model.number="desiredIdle"
              type="number"
              min="1"
              max="60"
              class="input input-sm w-full"
              :disabled="busy"
              required
          /></label>
          <label class="flex gap-2 items-center text-xs"
            ><input
              type="checkbox"
              class="checkbox checkbox-sm"
              v-model="desiredBlur"
              :disabled="busy"
            />{{ $t('private_lock.on_blur') }}</label
          >
        </template>
        <p class="text-xs text-warning">{{ $t('private_lock.scope') }}</p>
        <p v-if="validationError" role="alert" class="text-xs text-error">
          {{ validationError }}
        </p>
        <button type="submit" class="btn btn-primary btn-sm" :disabled="busy">
          {{ $t('private_lock.save') }}
        </button>
      </form>
    </ModalDialog>
  </div>
</template>
<script setup lang="ts">
import { ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import ModalDialog from './ModalDialog.vue';
const props = defineProps<{
  ready: boolean;
  enabled: boolean;
  locked: boolean;
  busy: boolean;
  error: string;
  idleMinutes: number;
  lockOnBlur: boolean;
  retryAfter: number;
  scope: string;
}>();
const emit = defineEmits(['unlock', 'configure', 'lock', 'retry']);
const { t } = useI18n();
const password = ref('');
const settingsOpen = ref(false);
const currentPassword = ref('');
const newPassword = ref('');
const confirmPassword = ref('');
const validationError = ref('');
const desiredEnabled = ref(props.enabled);
const desiredIdle = ref(props.idleMinutes);
const desiredBlur = ref(props.lockOnBlur);
function clearSettings() {
  settingsOpen.value = false;
  password.value = '';
  currentPassword.value = '';
  newPassword.value = '';
  confirmPassword.value = '';
  validationError.value = '';
}
function submitUnlock() {
  const value = password.value;
  password.value = '';
  emit('unlock', value);
}
function submitSettings() {
  validationError.value = '';
  if (desiredEnabled.value && newPassword.value !== confirmPassword.value) {
    validationError.value = t('private_lock.mismatch');
    return;
  }
  if (
    desiredEnabled.value &&
    ((!props.enabled && !newPassword.value) ||
      (newPassword.value &&
        !(
          Array.from(newPassword.value).length >= 8 &&
          Array.from(newPassword.value).length <= 128
        )))
  ) {
    validationError.value = t('private_lock.length');
    return;
  }
  const payload = {
    currentPassword: currentPassword.value,
    newPassword: newPassword.value || null,
    enabled: desiredEnabled.value,
    idleMinutes: desiredIdle.value,
    lockOnBlur: desiredBlur.value,
  };
  clearSettings();
  emit('configure', payload);
}
watch(
  () => [props.enabled, props.idleMinutes, props.lockOnBlur],
  () => {
    desiredEnabled.value = props.enabled;
    desiredIdle.value = props.idleMinutes;
    desiredBlur.value = props.lockOnBlur;
  },
);
watch(
  () => [props.scope, props.ready, props.locked],
  () => clearSettings(),
);
</script>

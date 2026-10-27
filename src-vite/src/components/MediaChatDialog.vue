<template>
  <Teleport to="body">
    <section v-show="mediaChat.open" ref="root" class="fixed z-[550] flex flex-col overflow-hidden rounded-box border border-base-content/20 bg-base-100 text-base-content shadow-2xl text-xs" :style="frameStyle" role="dialog" :aria-label="t('media_chat.title')" aria-modal="false" data-media-chat-dialog data-media-chat-drop @pointerdown="dismissActionsOutside" @keydown.stop="dialogKeydown" @dragenter.stop.prevent="dragOver = true" @dragover.stop.prevent="dragOver = true" @dragleave.stop="dragOver = false" @drop.stop.prevent="dropFiles" @lap-media-chat-files.stop="internalDrop">
      <header class="flex h-12 shrink-0 items-center gap-1 border-b border-base-content/10 px-3 cursor-move touch-none select-none" @pointerdown="beginFrameGesture($event, 'move')" @lostpointercapture="endFrameGesture" data-chat-header>
        <button class="btn btn-sm btn-square btn-ghost shrink-0" :title="t(showHistory ? 'media_chat.back_to_chat' : 'media_chat.history')" :aria-label="t(showHistory ? 'media_chat.back_to_chat' : 'media_chat.history')" :aria-expanded="showHistory" @pointerdown.stop @click="toggleHistory"><component :is="showHistory ? IconLeft : IconHistory" class="size-4" aria-hidden="true" /></button>
        <strong class="min-w-0 flex-1 truncate px-1 font-medium" :title="showHistory ? t('media_chat.history_title') : view?.session.title || t('media_chat.title')">{{ showHistory ? t('media_chat.history_title') : view?.session.title || t('media_chat.title') }}</strong>
        <span v-if="sending" class="loading loading-spinner loading-xs opacity-50" :title="t('media_chat.generating')"></span>
        <button class="btn btn-sm btn-square btn-ghost shrink-0" :disabled="loading" :title="t('media_chat.new_session')" :aria-label="t('media_chat.new_session')" @pointerdown.stop @click="newSession()"><IconAdd class="size-4" aria-hidden="true" /></button>
        <button class="btn btn-sm btn-square btn-ghost shrink-0" :title="t('media_chat.configure')" :aria-label="t('media_chat.configure')" @pointerdown.stop @click="sessionActionsOpen = false; historyMenuId = ''; openMediaAiSettings()"><IconSettings class="size-4" aria-hidden="true" /></button>
        <div v-if="!showHistory && view" class="relative shrink-0" data-chat-session-menu @pointerdown.stop>
          <button class="btn btn-sm btn-square btn-ghost" :title="t('media_chat.session_actions')" :aria-label="t('media_chat.session_actions')" :aria-expanded="sessionActionsOpen" @click="sessionActionsOpen = !sessionActionsOpen"><IconMore class="size-4" aria-hidden="true" /></button>
          <div v-if="sessionActionsOpen" class="absolute right-0 top-full z-40 mt-1 w-40 rounded-box border border-base-content/10 bg-base-100 p-1 shadow-xl" role="menu" :aria-label="t('media_chat.session_actions')">
            <button class="flex w-full items-center gap-2 rounded px-3 py-2 text-left hover:bg-base-200" role="menuitemcheckbox" :aria-checked="autoDockWindow" :title="t('media_chat.auto_dock_window_hint')" data-chat-auto-dock @click="toggleAutoDockWindow"><span class="inline-block size-4 shrink-0 text-center" aria-hidden="true">{{ autoDockWindow ? '✓' : '' }}</span>{{ t('media_chat.auto_dock_window') }}</button>
            <button class="flex w-full items-center gap-2 rounded px-3 py-2 text-left hover:bg-base-200" role="menuitem" @click="beginRename()"><IconRename class="size-4 opacity-60" />{{ t('media_chat.rename') }}</button>
            <button class="flex w-full items-center gap-2 rounded px-3 py-2 text-left text-error hover:bg-base-200 disabled:opacity-40" role="menuitem" :disabled="!!activeRequestId" @click="deleteSession()"><IconTrash class="size-4 opacity-60" />{{ t('media_chat.delete') }}</button>
          </div>
        </div>
        <button class="btn btn-sm btn-square btn-ghost shrink-0" :title="t('media_chat.close')" :aria-label="t('media_chat.close')" @pointerdown.stop @click="closeMediaChat"><IconClose class="size-4" aria-hidden="true" /></button>
      </header>
      <form v-if="renaming" class="flex shrink-0 items-center gap-2 border-b border-base-content/10 px-4 py-2" @submit.prevent="renameSession" data-chat-rename>
        <input ref="renameInput" v-model="renameText" class="input input-bordered input-sm min-w-0 flex-1" maxlength="120" :aria-label="t('media_chat.session_name')" :disabled="renamingBusy" @keydown.esc.stop.prevent="cancelRename" />
        <button class="btn btn-sm btn-ghost" type="submit" :disabled="renamingBusy || !renameText.trim()">{{ t('media_chat.rename') }}</button><button class="btn btn-sm btn-square btn-ghost" type="button" :aria-label="t('media_chat.dismiss')" :disabled="renamingBusy" @click="cancelRename"><IconClose class="size-4" /></button>
      </form>
      <p v-if="error" role="alert" class="shrink-0 px-3 py-2 text-error whitespace-pre-wrap break-words">{{ error }}</p>
      <p v-if="notice" role="status" class="shrink-0 px-3 py-1 text-warning break-words">{{ notice }}</p>
      <div class="flex min-h-0 flex-1 relative" :class="dragOver ? 'ring-2 ring-inset ring-primary' : ''">
        <div v-if="showHistory" class="flex min-w-0 min-h-0 flex-1 flex-col" data-chat-history>
          <label class="mx-5 mt-4 flex shrink-0 items-center gap-2 rounded-box border border-base-content/10 bg-base-200/40 px-3"><IconSearch class="size-4 shrink-0 opacity-40" aria-hidden="true" /><input v-model="historyQuery" class="input input-ghost input-sm min-w-0 w-full focus:outline-none" :placeholder="t('media_chat.search_chats')" :aria-label="t('media_chat.search_chats')" /></label>
          <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-5 pt-4">
            <p class="mb-2 px-3 text-[10px] font-medium text-base-content/45">{{ t('media_chat.recent_chats') }}</p>
            <div v-for="session in filteredSessions" :key="session.id" class="group relative flex items-center gap-2 rounded-box px-3 py-2.5 hover:bg-base-200/70" :class="session.id === view?.session.id ? 'bg-base-200/50' : ''" data-chat-history-row>
              <button class="min-w-0 flex-1 text-left" :aria-current="session.id === view?.session.id ? 'true' : undefined" :disabled="loading" @click="selectSession(session.id)"><span class="block truncate font-medium" :title="session.title">{{ session.title }}</span><span class="mt-1 block truncate text-[11px] text-base-content/45">{{ session.lastMessage || t('media_chat.empty_session') }}</span></button>
              <span v-if="session.runningRequestId" class="loading loading-dots loading-xs text-base-content/50" :aria-label="t('media_chat.generating')"></span>
              <div class="relative shrink-0" data-chat-session-menu>
                <button class="btn btn-sm btn-square btn-ghost opacity-0 group-hover:opacity-100 focus-visible:opacity-100" :class="historyMenuId === session.id ? 'opacity-100' : ''" :title="t('media_chat.session_actions')" :aria-label="t('media_chat.session_actions') + ': ' + session.title" :aria-expanded="historyMenuId === session.id" @click="toggleHistoryMenu($event, session.id)"><IconMore class="size-4" aria-hidden="true" /></button>
                <div v-if="historyMenuId === session.id" class="fixed z-[560] w-40 rounded-box border border-base-content/10 bg-base-100 p-1 shadow-xl" :style="historyMenuStyle" role="menu" :aria-label="t('media_chat.session_actions')">
                  <button class="flex w-full items-center gap-2 rounded px-3 py-2 text-left hover:bg-base-200" role="menuitem" @click="beginRename(session)"><IconRename class="size-4 opacity-60" />{{ t('media_chat.rename') }}</button>
                  <button class="flex w-full items-center gap-2 rounded px-3 py-2 text-left text-error hover:bg-base-200 disabled:opacity-40" role="menuitem" :disabled="!!session.runningRequestId || (request?.libraryId === activeLibrary && request?.sessionId === session.id)" @click="deleteSession(session)"><IconTrash class="size-4 opacity-60" />{{ t('media_chat.delete') }}</button>
                </div>
              </div>
            </div>
            <p v-if="!filteredSessions.length" class="px-3 py-12 text-center text-base-content/45">{{ t('media_chat.no_chats') }}</p>
          </div>
        </div>
        <main v-else class="relative flex flex-col min-w-0 min-h-0 flex-1">
          <div v-if="incomingIds.length" class="shrink-0 px-3 py-2 bg-base-200/70 space-y-1" data-chat-current-media><p>{{ t('media_chat.current_media', { count: incomingIds.length }) }}</p><p class="truncate opacity-60" :title="incoming.map(file => file.name).join('、')">{{ incoming.map(file => file.name).join('、') }}</p><div class="flex gap-1 flex-wrap"><button class="btn btn-xs" :disabled="loading || incomingIds.length > 16" @click="addIncoming">{{ t('media_chat.add_to_turn') }}</button><button class="btn btn-xs btn-ghost" :disabled="loading || incomingIds.length > 16" @click="newSession(incoming)">{{ t('media_chat.new_with_media') }}</button><button class="btn btn-xs btn-ghost" @click="incoming = []; incomingIds = []">{{ t('media_chat.dismiss') }}</button></div></div>
          <div class="relative flex-1 min-h-0">
          <div ref="messagesRoot" class="h-full overflow-y-auto px-5 py-5 space-y-6" data-chat-messages @scroll.passive="messagesScrolled">
            <p v-if="loading" class="loading loading-spinner loading-sm"></p>
            <div v-else-if="!renderedMessages.length" class="h-full min-h-32 flex flex-col items-center justify-center text-center gap-3 text-base-content/60"><strong class="text-base font-medium">{{ t('media_chat.empty_title') }}</strong><p>{{ t('media_chat.empty_hint') }}</p><p class="text-[11px]">{{ t('media_chat.drop_hint') }}</p></div>
            <article v-for="message in renderedMessages" :key="message.requestId ? `${message.requestId}:${message.role}` : message.id" class="min-w-0 space-y-2" :class="message.role === 'user' ? 'ml-auto w-fit max-w-[88%] rounded-box bg-base-200 px-4 py-3' : 'mr-auto w-full max-w-full py-1 pr-4'" :data-chat-message="message.id" :data-chat-role="message.role">
              <div class="flex justify-between gap-2 text-[10px] opacity-50"><span>{{ t(message.role === 'user' ? 'media_chat.you' : 'media_chat.assistant') }}<span v-if="message.role === 'assistant' && message.modelName"> · {{ message.modelName }}</span><span v-if="message.role === 'assistant' && message.reasoningEffort"> · {{ t('media_chat.reasoning') }}: {{ reasoningLabel(message.reasoningEffort) }}</span></span><time>{{ new Date(message.createdAt).toLocaleString() }}</time></div>
              <details v-if="message.role === 'assistant' && message.reasoningContent?.trim()" class="rounded-box border border-base-content/10 px-3 py-2 text-base-content/65" data-chat-reasoning-summary><summary class="cursor-pointer select-none text-[11px]">{{ t('media_chat.reasoning_summary') }}</summary><p class="mt-2 whitespace-pre-wrap break-words leading-relaxed select-text">{{ message.reasoningContent }}</p></details>
              <template v-if="message.content">
                <MediaChatMarkdown v-if="message.role === 'assistant'" :content="message.content" :streaming="message.status === 'pending'" data-chat-message-content @link-error="error = $event" @rendered="markdownRendered" />
                <p v-else class="whitespace-pre-wrap break-words leading-relaxed select-text" data-chat-message-content>{{ message.content }}</p>
              </template>
              <p v-if="message.role === 'assistant' && (message.status === 'pending' || message.status === 'failed' || message.status === 'cancelled')" class="flex items-center gap-2 text-[11px]" :class="message.status === 'failed' ? 'text-error' : 'text-base-content/50'" role="status" :data-chat-phase="message.phase || (message.status === 'pending' ? 'waiting' : message.status)"><span v-if="message.status === 'pending'" class="loading loading-dots loading-xs"></span>{{ t('media_chat.phase_' + (message.phase || (message.status === 'pending' ? 'waiting' : message.status))) }}</p>
              <p v-if="message.error" class="text-error whitespace-pre-wrap break-words">{{ message.error }}</p>
              <div v-if="message.toolCalls?.length" class="space-y-2" data-chat-tool-calls>
                <details v-for="call in message.toolCalls" :key="call.callId" class="group/tool min-w-0 rounded-box border border-base-content/10 bg-base-200/30 px-3 py-2" :data-chat-tool-call="call.callId" :data-tool-status="call.status">
                  <summary class="flex cursor-pointer list-none items-center gap-2 text-[11px]"><span v-if="call.status === 'running'" class="loading loading-spinner loading-xs opacity-50"></span><IconFileInfo v-else class="size-3.5 shrink-0 opacity-50" aria-hidden="true" /><span class="min-w-0 flex-1 truncate">{{ toolLabel(call.name) }}</span><span class="shrink-0" :class="call.status === 'failed' ? 'text-error' : 'opacity-50'">{{ t('media_chat.tool_status_' + call.status) }}</span><IconArrowDown class="size-3 shrink-0 opacity-50 transition-transform group-open/tool:rotate-180" aria-hidden="true" /></summary>
                  <div class="mt-2 min-w-0 space-y-3 text-[11px]">
                    <p v-if="call.error" class="whitespace-pre-wrap break-words text-error">{{ call.error }}</p>
                    <p v-else class="text-base-content/60">{{ toolResultSummary(call) }}</p>
                    <p class="break-all font-mono text-[10px] text-base-content/45" data-chat-tool-identity>{{ call.name }} · {{ call.callId }}</p>
                    <section class="min-w-0 space-y-1" data-chat-tool-arguments><h4 class="font-medium text-base-content/65">{{ t('media_chat.tool_arguments') }}</h4><pre class="max-h-64 overflow-auto whitespace-pre-wrap break-all rounded bg-base-100 p-2 font-mono text-[10px] leading-relaxed select-text">{{ formatToolJson(call.arguments) }}</pre></section>
                    <section class="min-w-0 space-y-1" data-chat-tool-result><h4 class="font-medium text-base-content/65">{{ t('media_chat.tool_result') }}</h4><pre v-if="call.result !== null && call.result !== undefined" class="max-h-64 overflow-auto whitespace-pre-wrap break-all rounded bg-base-100 p-2 font-mono text-[10px] leading-relaxed select-text">{{ formatToolJson(call.result) }}</pre><p v-else class="text-base-content/45">{{ t(call.status === 'running' ? 'media_chat.tool_awaiting_result' : 'media_chat.tool_empty_result') }}</p></section>
                    <div v-if="toolTask(call)" class="rounded-box border border-base-content/10 bg-base-100 p-2" :data-chat-tool-task="toolTask(call)!.id"><p class="flex flex-wrap items-center justify-between gap-2"><span>{{ taskStateLabel(toolTask(call)!.state) }}</span><span>{{ toolTask(call)!.completed }} / {{ toolTask(call)!.total }}</span></p><p v-if="toolTask(call)!.failed" class="mt-1 text-error">{{ t('media_chat.tool_task_failed', { count: toolTask(call)!.failed }) }}</p><p v-if="toolTaskUnrefreshed(call)" class="mt-1 text-base-content/55">{{ t('media_chat.tool_task_not_refreshed') }}</p><button class="btn btn-xs btn-ghost mt-2" @click="openTasks(false)">{{ t('media_chat.tool_view_tasks') }}</button></div>
                  </div>
                </details>
              </div>
              <div v-if="messageProposals(message).length" class="space-y-2">
                <section v-for="call in messageProposals(message)" :key="toolProposal(call)!.id" class="rounded-box border border-primary/20 bg-primary/5 p-3 space-y-2" :data-chat-proposal="toolProposal(call)!.id">
                  <div class="flex flex-wrap items-center justify-between gap-2"><strong class="font-medium">{{ t('media_chat.chat_suggestion') }}</strong><button class="btn btn-sm btn-primary" :disabled="applying || reviewingToolProposal" :data-chat-tool-proposal="toolProposal(call)!.id" @click="reviewToolProposal(call)">{{ t('media_chat.tool_review_proposal') }}</button></div>
                  <p class="text-[11px] text-base-content/65">{{ t('media_chat.tool_proposal_count', { count: toolProposal(call)!.files.length }) }}</p>
                  <div class="flex flex-wrap gap-1"><span v-for="field in adoptionFields.filter(field => toolProposal(call)!.fields[field])" :key="field" class="badge badge-sm badge-ghost">{{ t('media_chat.field_' + field) }}</span></div>
                </section>
              </div>
              <div v-if="message.references.length" class="flex flex-wrap gap-2"><div v-for="(file, i) in message.references" :key="file.fileId + ':' + file.sourceStamp" class="relative group rounded-box border border-base-content/10 bg-base-100 min-w-0 max-w-64" data-chat-reference-card><button class="flex items-center gap-2 p-2 pr-8 w-full min-w-0 text-left hover:bg-base-200 rounded-box" :title="t('media_chat.view_media', { name: file.name })" @click="openFile(file)"><img v-if="thumbnails[file.fileId]" :src="thumbnails[file.fileId]" class="h-12 w-12 rounded object-cover shrink-0" alt="" /><IconFile v-else class="h-8 w-8 m-2 shrink-0 opacity-40" aria-hidden="true" /><span class="min-w-0"><span class="block truncate font-medium">[{{ i + 1 }}] {{ file.name }}</span><span class="block text-[10px] opacity-50">{{ mediaSummary(file) }}</span></span></button><button class="absolute right-1 top-1 btn btn-xs btn-square btn-ghost" :aria-label="t('media_chat.attach_again', { name: file.name })" :disabled="attachments.some(item => item.fileId === file.fileId)" @click="attachIds([file.fileId])">＋</button></div></div>
              <div v-else-if="message.role === 'user' && message.id === streamTurn?.user.id && streamTurn.media.length" class="flex flex-wrap gap-2"><button v-for="file in streamTurn.media" :key="file.fileId" class="flex min-w-0 max-w-64 items-center gap-2 rounded-box border border-base-content/10 bg-base-100 p-2 text-left hover:bg-base-200" data-chat-pending-reference :title="t('media_chat.view_media', { name: file.name })" @click="openFile(file)"><img v-if="thumbnails[file.fileId]" :src="thumbnails[file.fileId]" class="size-10 shrink-0 rounded object-cover" alt="" /><IconFile v-else class="size-6 shrink-0 opacity-40" aria-hidden="true" /><span class="truncate">{{ file.name }}</span></button></div>
            </article>
          </div>
          <button v-if="newContent" class="absolute bottom-0 left-1/2 z-10 mb-3 -translate-x-1/2 btn btn-sm gap-2 rounded-full border-base-content/10 bg-base-100 shadow-lg" data-chat-new-content @click="scrollMessages(true)"><IconArrowDown class="size-4" aria-hidden="true" />{{ t('media_chat.new_content') }}</button>
          </div>
          <div class="relative shrink-0 border-t border-base-content/10 p-3 space-y-2">
            <div v-if="contextOpen" class="absolute bottom-full left-3 right-3 z-30 mb-2 max-h-[min(24rem,50vh)] overflow-y-auto rounded-box border border-base-content/15 bg-base-100 p-3 shadow-xl space-y-3" data-chat-context data-chat-context-panel @keydown.esc.stop.prevent="contextOpen = false" @keydown.stop>
              <div class="flex items-center justify-between gap-2"><strong class="font-medium">{{ t('media_chat.context_tools') }}</strong><button class="btn btn-xs btn-square btn-ghost" :aria-label="t('media_chat.dismiss')" @click="contextOpen = false"><IconClose class="size-3.5" /></button></div>
              <label class="flex items-center gap-2"><input v-model="toolsEnabled" class="checkbox checkbox-xs" type="checkbox" data-chat-tools-enabled />{{ t('media_chat.tools_enabled') }}</label><p class="text-[11px] text-base-content/55">{{ t('media_chat.tools_enabled_hint') }}</p>
              <label class="flex items-center gap-3"><span class="shrink-0">{{ t('media_chat.tool_scope') }}</span><select v-model="toolScope" class="select select-bordered select-sm min-w-0 flex-1" :disabled="!toolsEnabled" :aria-label="t('media_chat.tool_scope')" data-chat-tool-scope><option :value="null">{{ t('media_chat.scope_auto') }} · {{ t(attachments.length ? 'media_chat.scope_references' : 'media_chat.scope_library') }}</option><option value="references">{{ t('media_chat.scope_references') }}</option><option value="library">{{ t('media_chat.scope_library') }}</option></select></label>
              <p class="text-[11px] text-base-content/55">{{ t('media_chat.context_scope_hint') }}</p>
              <fieldset class="space-y-2"><legend class="mb-2 font-medium">{{ t('media_chat.context_fields') }}</legend><div class="grid grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-4"><label v-for="field in CHAT_CONTEXT_FIELDS" :key="field" class="flex items-center gap-2"><input v-model="contextFields" type="checkbox" class="checkbox checkbox-xs" :value="field" :data-chat-context-field="field" />{{ t('media_chat.context_' + field) }}</label></div></fieldset>
              <p class="text-[11px] text-base-content/55">{{ t('media_chat.context_fields_hint') }}</p>
              <p class="text-[11px] text-base-content/55">{{ t('media_chat.context_location_hint') }}</p><p class="text-[11px] text-base-content/55">{{ t('media_chat.context_history_hint') }}</p>
            </div>
            <div v-if="attachments.length" class="space-y-1"><span class="opacity-50">{{ t('media_chat.turn_media', { count: attachments.length }) }}</span><div class="flex gap-2 overflow-x-auto pb-1"><div v-for="file in attachments" :key="file.fileId" class="relative shrink-0 w-52 rounded-box border border-base-content/10 bg-base-200/50" data-chat-attachment-card><button class="flex gap-2 items-center p-2 pr-7 w-full min-w-0 text-left hover:bg-base-200 rounded-box" :title="t('media_chat.view_media', { name: file.name })" @click="openFile(file)"><img v-if="thumbnails[file.fileId]" :src="thumbnails[file.fileId]" class="h-12 w-12 rounded object-cover shrink-0" alt="" /><IconFile v-else class="h-8 w-8 m-2 shrink-0 opacity-40" aria-hidden="true" /><span class="min-w-0"><span class="block truncate font-medium">{{ file.name }}</span><span class="block text-[10px] opacity-50">{{ mediaSummary(file) }}</span></span></button><button type="button" class="btn btn-xs btn-square btn-ghost absolute right-0.5 top-0.5" :aria-label="t('media_chat.remove_attachment', { name: file.name })" @click="removeAttachment(file.fileId)">×</button></div></div></div>
            <p v-if="!readyModels.length" class="text-warning">{{ t('media_chat.no_ready_model') }}</p>
            <p v-else-if="selectedModelId && !readyModels.some(model => model.id === selectedModelId)" class="text-warning">{{ t('media_chat.choose_compatible_model') }}</p>
            <div class="relative">
              <div v-if="pickerOpen || mention" class="absolute z-30 bottom-full left-0 mb-2 w-full rounded-box border border-base-content/20 bg-base-100 shadow-xl p-2 space-y-2" role="listbox" :aria-label="t('media_chat.choose_media')" data-chat-mentions>
                <div class="flex gap-1"><button class="btn btn-xs" :class="candidateSource === 'session' ? 'btn-active' : ''" @mousedown.prevent @click="candidateSource = 'session'">{{ t('media_chat.from_session') }}</button><button class="btn btn-xs" :class="candidateSource === 'library' ? 'btn-active' : ''" @mousedown.prevent @click="candidateSource = 'library'">{{ t('media_chat.from_library') }}</button><button class="btn btn-xs btn-ghost ml-auto" @mousedown.prevent @click="closePicker">×</button></div>
                <input v-if="pickerOpen && !mention" ref="pickerSearch" v-model="pickerQuery" class="input input-bordered input-xs w-full" :placeholder="t('media_chat.search_media')" @keydown="candidateKeydown" />
                <div class="max-h-52 overflow-y-auto"><p v-if="candidateLoading" class="loading loading-spinner loading-xs"></p><button v-for="(file, i) in candidates" :key="file.fileId" class="flex gap-2 items-center rounded p-2 w-full min-w-0 text-left hover:bg-base-200" :class="i === candidateIndex ? 'bg-base-200' : ''" role="option" :aria-selected="i === candidateIndex" @mousedown.prevent @click="pickCandidate(file)"><img v-if="thumbnails[file.fileId]" :src="thumbnails[file.fileId]" class="w-9 h-9 shrink-0 rounded object-cover" alt="" /><span class="min-w-0 flex-1"><span class="block truncate">{{ file.name }}</span><span class="block truncate opacity-50 text-[10px]">{{ file.folderPath }}<span v-if="file.fileType === 2"> · {{ mediaTime(file.duration) }}</span></span></span><span v-if="file.cloudOnly" class="text-warning">{{ t('media_chat.cloud_only') }}</span></button><p v-if="!candidateLoading && !candidates.length" class="p-2 opacity-50">{{ t('media_chat.no_media') }}</p></div>
              </div>
              <textarea ref="composer" v-model="draft" rows="3" maxlength="8000" class="textarea textarea-bordered w-full resize-none leading-relaxed" :placeholder="t('media_chat.composer_placeholder')" :aria-label="t('media_chat.question')" @input="updateMention" @click="updateMention" @keyup="composerKeyup" @keydown="composerKeydown"></textarea>
            </div>
            <div class="flex flex-wrap items-center gap-2" data-chat-composer-controls>
              <div class="relative" data-chat-actions><button class="btn btn-sm btn-circle btn-ghost" :aria-label="t('media_chat.actions')" :aria-expanded="actionsOpen" @click="toggleActions">＋</button><div v-if="actionsOpen" class="absolute bottom-full left-0 mb-2 z-30 w-56 max-h-72 overflow-y-auto rounded-box border border-base-content/15 bg-base-100 shadow-xl p-1" role="menu" :aria-label="t('media_chat.actions')" data-chat-actions-menu><button v-for="(item, i) in menuItems" :key="item.id" type="button" class="block w-full rounded px-3 py-2 text-left hover:bg-base-200 disabled:opacity-40" :class="i === actionIndex ? 'bg-base-200' : ''" role="menuitem" :disabled="item.disabled" @click="runMenuAction(item.id)">{{ t('media_chat.' + item.label) }}</button></div></div>
              <select v-model="selectedModelId" class="select select-ghost select-sm min-w-0 flex-1 max-w-72" :aria-label="t('media_chat.model')"><option value="">{{ t('media_chat.choose_model') }}</option><option v-for="model in readyModels" :key="model.id" :value="model.id">{{ modelName(model) }}</option></select>
              <button class="btn btn-sm btn-ghost gap-1.5" :aria-label="t('media_chat.context_tools')" :aria-expanded="contextOpen" :title="t('media_chat.context_tools')" data-chat-context data-chat-context-toggle @click="toggleContext"><IconFileInfo class="size-4" aria-hidden="true" /><span>{{ t('media_chat.context_tools') }}</span></button>
              <select v-if="selectedModelId" v-model="reasoningEffort" class="select select-ghost select-sm min-w-0 max-w-48" :disabled="!reasoningLevels.length" :aria-label="t('media_chat.reasoning')" data-chat-reasoning><option :value="null">{{ reasoningDefaultLabel }}</option><option v-if="defaultReasoningEffort" value="">{{ t('media_chat.reasoning_auto') }}</option><option v-for="level in reasoningLevels" :key="level" :value="level">{{ reasoningLabel(level) }}</option></select>
              <button v-if="activeRequestId" class="btn btn-sm ml-auto" @click="stopAnswer">{{ t('media_chat.stop') }}</button><button class="btn btn-sm btn-primary" :class="activeRequestId ? '' : 'ml-auto'" :disabled="!canSend" @click="send">{{ t('media_chat.send') }}</button>
            </div>
            <p class="text-[10px] opacity-50">{{ t('media_chat.scope_hint') }} {{ t('media_chat.close_hint') }}</p>
          </div>
        </main>
      </div>
      <div v-for="edge in CHAT_RESIZE_EDGES" :key="edge" class="absolute z-20 touch-none select-none outline-none focus-visible:bg-primary/15" :style="resizeHandleStyle(edge)" role="button" tabindex="0" :aria-label="t('media_chat.resize_' + edge)" :data-chat-resize="edge" @pointerdown.stop="beginFrameGesture($event, 'resize', edge)" @lostpointercapture="endFrameGesture" @keydown.stop="resizeKeydown($event, edge)"></div>
    </section>
    <ModalDialog v-if="previewOpen" :key="viewport.width + ':' + viewport.height" :title="t('media_chat.metadata_preview')" :width="previewSize.width" opaque @cancel="previewOpen = false">
      <fieldset class="metadata-preview flex min-h-0 min-w-0 flex-col gap-3 text-sm text-base-content" :style="{ maxHeight: previewSize.maxHeight + 'px' }" :disabled="applying" data-chat-adoption-preview @keydown.esc.stop.prevent="previewOpen = false" @keydown.stop>
        <div class="shrink-0 space-y-2">
          <p class="text-base-content/80">{{ t('media_chat.preview_hint') }}</p>
          <div v-if="previewRows.length === 1" class="flex min-w-0 items-center gap-3" :data-chat-preview-media="previewRows[0].file.fileId">
            <button class="flex min-w-0 flex-1 items-center gap-2 text-left" :title="t('media_chat.view_media', { name: previewRows[0].file.name })" @click="openFile(previewRows[0].file)"><img v-if="thumbnails[previewRows[0].file.fileId]" :src="thumbnails[previewRows[0].file.fileId]" class="size-10 shrink-0 rounded object-cover" alt="" /><IconFile v-else class="size-8 shrink-0" /><span class="min-w-0"><strong class="block truncate" :title="previewRows[0].file.name">{{ previewRows[0].file.name }}</strong><span class="text-xs text-base-content/80">{{ mediaSummary(previewRows[0].file) }}</span></span></button>
            <span :class="previewRows[0].error ? 'text-error' : previewRows[0].applied ? 'text-success' : 'text-base-content'" role="status">{{ previewStatus(previewRows[0]) }}</span>
          </div>
          <div class="flex flex-wrap items-center gap-x-5 gap-y-2" data-chat-preview-policies>
            <label v-if="previewHasText" class="flex items-center gap-2"><input v-model="onlyEmpty" type="checkbox" class="checkbox checkbox-xs" />{{ t('media_chat.only_empty') }}</label>
            <span v-if="previewHasTags" class="text-base-content/80">{{ t('media_chat.preview_append_tags') }}</span>
          </div>
          <p v-if="reviewingToolProposal" class="flex items-center gap-2 text-base-content/80" role="status"><span class="loading loading-spinner loading-xs"></span>{{ t('media_chat.preparing_suggestions') }}</p>
          <p v-if="previewHasTags && !reviewingToolProposal && previewTagCatalog === null" class="text-base-content/80">{{ t('media_chat.preview_catalog_unknown') }}</p>
          <p v-if="previewError" role="alert" class="text-error whitespace-pre-wrap">{{ previewError }}</p>
        </div>
        <div class="min-h-0 overflow-auto rounded-box border border-base-content/15 bg-base-100" data-chat-preview-scroll>
          <table class="table table-sm w-full min-w-[600px] table-fixed text-sm" data-chat-preview-table>
            <colgroup><col class="w-10" /><col class="w-20" /><col class="w-[32%]" /><col /></colgroup>
            <thead class="sticky top-0 z-10 bg-base-200 text-sm text-base-content"><tr><th class="text-center"><span class="sr-only">{{ t('media_chat.preview_select') }}</span></th><th>{{ t('media_chat.preview_field') }}</th><th>{{ t('media_chat.current_value') }}</th><th>{{ t('media_chat.preview_suggestion') }}</th></tr></thead>
            <tbody v-for="row in previewRows" :key="row.file.fileId" :data-chat-preview-group="row.file.fileId">
              <tr v-if="previewRows.length > 1" class="bg-base-200"><th colspan="4" class="py-2 font-normal"><div class="flex items-center gap-3" :data-chat-preview-media="row.file.fileId"><button class="flex min-w-0 flex-1 items-center gap-2 text-left" :title="t('media_chat.view_media', { name: row.file.name })" @click="openFile(row.file)"><img v-if="thumbnails[row.file.fileId]" :src="thumbnails[row.file.fileId]" class="size-8 shrink-0 rounded object-cover" alt="" /><IconFile v-else class="size-6 shrink-0" /><strong class="min-w-0 truncate" :title="row.file.name">{{ row.file.name }}</strong></button><span class="shrink-0" :class="row.error ? 'text-error' : row.applied ? 'text-success' : 'text-base-content'" role="status">{{ previewStatus(row) }}</span></div></th></tr>
              <tr v-if="row.analysisId && row.analyses.length > 1"><td colspan="4"><select v-model="row.analysisId" class="select select-bordered select-xs w-full" :disabled="row.applied || !row.ready" :aria-label="t('media_chat.preview_segment')" @change="selectPreviewAnalysis(row)"><option v-for="analysis in row.analyses" :key="analysis.id" :value="analysis.id">{{ mediaTime(analysis.start) }}–{{ mediaTime(analysis.end) }} · {{ analysis.title }}</option></select></td></tr>
              <tr v-for="field in previewVisibleFields(row)" :key="field" class="align-top" :data-chat-preview-field="field" :data-preview-selected="row.fields[field]">
                <td class="pt-3 text-center"><input v-model="row.fields[field]" type="checkbox" class="checkbox checkbox-xs" :disabled="!row.ready || row.applied || previewDismissed(row).includes(field)" :aria-label="t('media_chat.preview_select_field', { field: t('media_chat.field_' + field), name: row.file.name })" /></td>
                <th class="py-3 font-medium"><span>{{ t('media_chat.field_' + field) }}</span><span v-if="previewDismissed(row).includes(field)" class="mt-1 block text-[10px] font-normal text-warning">{{ t('media_ai.dismissed') }}</span></th>
                <td class="py-2 text-base-content">
                  <div v-if="field === 'tags'" class="flex flex-wrap gap-1"><span v-for="tag in row.currentTags" :key="tag" class="badge badge-sm h-auto max-w-full whitespace-normal break-all border-base-content/20 bg-base-200 py-1 text-sm text-base-content">{{ tag }}</span><span v-if="!row.currentTags.length" class="py-1">—</span></div>
                  <p v-else class="whitespace-pre-wrap break-words py-1" :class="field === 'description' && !row.expandedDescription ? 'line-clamp-2' : ''">{{ (field === 'title' ? row.source?.title : row.source?.comments) || '—' }}</p>
                  <span v-if="field !== 'tags' && onlyEmpty && (field === 'title' ? row.source?.title?.trim() : row.source?.comments?.trim())" class="mt-1 block text-[10px] text-warning">{{ t('media_chat.preview_keep_existing') }}</span>
                </td>
                <td class="py-2">
                  <input v-if="field === 'title'" v-model="row.proposal.title" class="input input-bordered input-sm w-full min-w-0" maxlength="256" :disabled="!row.ready || row.applied || previewDismissed(row).includes(field)" :aria-label="t('media_chat.field_title')" />
                  <div v-else-if="field === 'description'" class="space-y-1"><textarea v-model="row.proposal.description" :rows="row.expandedDescription ? 7 : 2" class="textarea textarea-bordered w-full min-w-0" maxlength="8000" :disabled="!row.ready || row.applied || previewDismissed(row).includes(field)" :aria-label="t('media_chat.field_description')"></textarea><button class="btn btn-xs btn-ghost" :aria-expanded="!!row.expandedDescription" @click="row.expandedDescription = !row.expandedDescription">{{ t(row.expandedDescription ? 'media_chat.preview_collapse' : 'media_chat.preview_expand') }}</button></div>
                  <div v-else class="space-y-1">
                    <div class="flex min-h-9 flex-wrap items-center gap-1 rounded-field border border-base-content/20 bg-base-100 p-1 focus-within:outline-2 focus-within:outline-primary" data-chat-preview-tags>
                      <span v-for="(tag, index) in previewTags(row)" :key="index" class="flex max-w-full items-center gap-1 rounded border border-base-content/20 bg-base-200 px-1.5 py-0.5 text-sm text-base-content" :data-tag-state="previewTagState(row, tag)"><input :value="previewTagEditValue(row, tag)" :style="{ width: previewTagInputWidth(previewTagEditValue(row, tag)) + 'ch' }" class="min-w-0 max-w-full bg-transparent py-1 font-medium text-base-content outline-none focus-visible:outline-2 focus-visible:outline-primary disabled:cursor-not-allowed disabled:opacity-60" :title="tag" :aria-label="t('media_chat.preview_edit_tag', { tag })" :disabled="!row.ready || row.applied || previewDismissed(row).includes(field)" @input="editPreviewTag(row, tag, ($event.target as HTMLInputElement).value)" @keydown="previewTagEditKeydown(row, $event)" @blur="addPreviewTags(row)" /><span v-if="previewTagState(row, tag) === 'new'" class="shrink-0 text-[11px] text-base-content/80">{{ t('media_chat.preview_new_tag') }}</span><button class="rounded p-0.5 text-base-content hover:bg-base-300 focus-visible:outline-2 focus-visible:outline-primary disabled:cursor-not-allowed disabled:opacity-40" :disabled="!row.ready || row.applied || previewDismissed(row).includes(field)" :aria-label="t('media_chat.preview_remove_tag', { tag })" @click="removePreviewTag(row, tag)"><IconClose class="size-3.5" aria-hidden="true" /></button></span>
                      <input v-model="row.tagDraft" class="h-8 min-w-24 flex-1 bg-transparent px-1 text-sm text-base-content outline-none placeholder:text-base-content/70 disabled:cursor-not-allowed disabled:opacity-60" :disabled="!row.ready || row.applied || previewDismissed(row).includes(field)" :aria-label="t('media_chat.preview_add_tag')" :placeholder="t('media_chat.preview_add_tag')" @keydown="previewTagKeydown(row, $event)" @blur="addPreviewTags(row)" />
                    </div>
                    <p v-if="previewTagValidation(row)" class="text-error" role="alert">{{ previewTagValidation(row) }}</p>
                  </div>
                </td>
              </tr>
              <tr v-if="row.error"><td colspan="4" class="text-error whitespace-pre-wrap break-words" role="alert">{{ row.error }}</td></tr>
              <tr v-if="row.applied"><td colspan="4" class="text-success">{{ t('media_chat.applied') }}</td></tr>
            </tbody>
          </table>
        </div>
        <div class="flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-base-content/10 pt-3" data-chat-preview-footer>
          <p class="text-base-content" role="status">{{ t('media_chat.preview_selection', { fields: previewSelectedFieldCount, media: adoptionCount }) }}</p>
          <div class="flex items-center gap-2"><button class="btn btn-sm" @click="previewOpen = false">{{ t('media_chat.preview_close') }}</button><button class="btn btn-sm btn-primary" :disabled="applying || reviewingToolProposal || !adoptionCount" @click="applySuggestions"><span v-if="applying" class="loading loading-spinner loading-xs"></span>{{ t('media_chat.apply_to_media', { count: adoptionCount }) }}</button></div>
        </div>
      </fieldset>
    </ModalDialog>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import { Channel, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { useUIStore } from '@/stores/uiStore';
import { getAllTags, getDragPayload, getFileThumbById, getTagsForFile } from '@/common/api';
import { getExternalDropPaths } from '@/common/externalDrop';
import { getThumbnailDataUrl, getThumbUrl, isWin, isMac } from '@/common/utils';
import { mediaTime, openMediaAiBatch, openMediaAiSettings, type MediaCapability, type MediaConfiguration, type MediaModel } from '@/common/mediaAi';
import { mediaChat, closeMediaChat, validMediaChatIds, CHAT_CONTEXT_FIELDS, normalizeChatContextFields, type ChatCandidate, type ChatContextField, type ChatMessage, type ChatReference, type ChatSession, type ChatSessionView, type ChatSource, type ChatStreamEvent, type ChatToolCall, type ChatToolScope } from '@/common/mediaChat';
import { openMediaChatMedia } from '@/common/mediaChatMedia';
import { CHAT_RESIZE_EDGES, captureChatDockAnchor, dockChatFrame, fitChatFrame, insertMediaMention, isChatNearBottom, mentionAtCaret, normalizeChatDockAnchor, resizeChatFrame, slashAtCaret, type ChatDockAnchor, type ChatFrame, type ChatResizeEdge, type MentionRange } from '@/common/mediaChatUi';
import { IconAdd, IconArrowDown, IconClose, IconFile, IconFileInfo, IconHistory, IconLeft, IconMore, IconRename, IconSearch, IconSettings, IconTrash } from '@/common/icons';
import { pendingChanges } from '@/common/pendingChanges';
import ModalDialog from './ModalDialog.vue';
import MediaChatMarkdown from './MediaChatMarkdown.vue';

const { t, te } = useI18n(), uiStore = useUIStore();
const root = ref<HTMLElement | null>(null), composer = ref<HTMLTextAreaElement | null>(null), messagesRoot = ref<HTMLElement | null>(null);
const pickerSearch = ref<HTMLInputElement | null>(null), renameInput = ref<HTMLInputElement | null>(null);
const viewport = ref({ width: window.innerWidth, height: window.innerHeight });
const previewSize = computed(() => ({ width: Math.max(1, Math.min(previewRows.value.length > 1 ? 1040 : 860, viewport.value.width - 24)), maxHeight: Math.max(1, viewport.value.height - 100) }));
const frame = ref<ChatFrame>({ x: 12, y: 12, width: 1000, height: 680 });
const frameStyle = computed(() => ({ left: `${frame.value.x}px`, top: `${frame.value.y}px`, width: `${frame.value.width}px`, height: `${frame.value.height}px` }));
const autoDockWindow = computed(() => config.settings.mediaChat?.autoDockWindow === true);
let dockAnchor: ChatDockAnchor | null = null, frameReady = false;
const showHistory = ref(false), historyQuery = ref(''), historyMenuId = ref(''), sessionActionsOpen = ref(false);
const historyMenuStyle = ref<Record<string, string>>({});
const sessions = ref<ChatSession[]>([]), view = ref<ChatSessionView | null>(null), activeLibrary = ref(''), configuration = ref<MediaConfiguration | null>(null);
const filteredSessions = computed(() => { const query = historyQuery.value.trim().toLocaleLowerCase(); return sessions.value.filter(session => !query || `${session.title} ${session.lastMessage}`.toLocaleLowerCase().includes(query)); });
const loading = ref(false), error = ref(''), notice = ref(''), draft = ref(''), attachments = ref<ChatCandidate[]>([]), incoming = ref<ChatCandidate[]>([]), incomingIds = ref<number[]>([]);
const visual = ref(true), selectedModelId = ref(''), renaming = ref(false), renameText = ref('');
const renameTarget = ref<{ libraryId: string; sessionId: string } | null>(null), renamingBusy = ref(false);
const nearBottom = ref(true), newContent = ref(false);
let scrollRevision = 0;
const reasoningEffort = ref<string | null>(null), actionsOpen = ref(false), actionIndex = ref(0);
const contextOpen = ref(false), toolsEnabled = ref(true), toolScope = ref<ChatToolScope | null>(null), contextFields = ref<ChatContextField[]>(normalizeChatContextFields(undefined));
let actionSlash: number | null = null;
const request = ref<{ id: string; sessionId: string; libraryId: string } | null>(null);
const sending = computed(() => !!request.value);
const activeRequestId = computed(() => view.value?.session.runningRequestId || (request.value?.libraryId === activeLibrary.value && request.value?.sessionId === view.value?.session.id ? request.value?.id : null));
type StreamTurn = { libraryId: string; sessionId: string; requestId: string; seq: number; closed: boolean; cancelRequested: boolean; registeredCancelSent: boolean; user: ChatMessage; assistant: ChatMessage; media: ChatCandidate[]; finalView?: ChatSessionView };
const streamTurn = ref<StreamTurn | null>(null);
const renderedMessages = computed(() => {
  const messages = view.value?.messages || [], turn = streamTurn.value;
  if (!turn || turn.libraryId !== activeLibrary.value || turn.sessionId !== view.value?.session.id || turn.finalView) return messages;
  const previous = messages.filter(message => message.requestId !== turn.requestId);
  const user = messages.find(message => message.requestId === turn.requestId && message.role === 'user') || turn.user;
  const saved = messages.find(message => message.requestId === turn.requestId && message.role === 'assistant');
  return [...previous, user, saved ? { ...saved, ...turn.assistant, id: saved.id, references: saved.references, error: turn.assistant.error || saved.error } : turn.assistant];
});
type ToolTaskSummary = { id: string; state: string; completed: number; total: number; failed: number };
const toolTaskStates = ref<Record<string, { callId: string; task: ToolTaskSummary; missing: boolean }>>({});
const currentToolTasks = computed(() => {
  const tasks = new Map<string, { call: ChatToolCall; task: ToolTaskSummary }>();
  for (const message of renderedMessages.value) for (const call of message.toolCalls || []) {
    const task = toolTaskSnapshot(call);
    if (task) tasks.set(task.id, { call, task });
  }
  return tasks;
});
let toolTaskReadGeneration = 0, toolTaskLoading: { epoch: number; libraryId: string; sessionId: string } | null = null, offToolTasks: (() => void) | undefined;
const thumbnails = ref<Record<number, string>>({});
const mention = ref<MentionRange | null>(null), pickerOpen = ref(false), pickerQuery = ref(''), candidateSource = ref<'session' | 'library'>('library'), candidates = ref<ChatCandidate[]>([]), candidateIndex = ref(0), candidateLoading = ref(false), dragOver = ref(false);
const drafts = new Map<string, { text: string; attachments: ChatCandidate[]; modelId: string; reasoningEffort: string | null; toolsEnabled: boolean; toolScope: ChatToolScope | null; contextFields: ChatContextField[]; revision: number }>();
let draftRevision = 0;
let disposed = false, generation = 0, candidateGeneration = 0, messageGeneration = 0, sessionListGeneration = 0, picking = false, inputRegistered = false, candidateTimer: ReturnType<typeof setTimeout> | undefined;
let refreshTimer: ReturnType<typeof setInterval> | undefined;
const sessionMedia = computed(() => {
  const files = new Map<number, ChatCandidate>();
  for (const message of view.value?.messages || []) for (const file of message.references) files.set(file.fileId, { fileId: file.fileId, name: file.name, folderPath: file.source.path.replace(/[\\/][^\\/]*$/, ''), fileType: file.fileType, width: file.source.width, height: file.source.height, duration: file.source.duration, cloudOnly: file.source.cloudOnly, hasAnalysis: file.evidence.length > 0, sourceStamp: file.sourceStamp });
  for (const file of attachments.value) files.set(file.fileId, file);
  return [...files.values()];
});
function chatCapabilities(pixels: boolean, files = attachments.value): MediaCapability[] {
  return pixels && files.length ? [...new Set(files.map(file => file.fileType === 2 ? 'video_frames' as const : 'image' as const))] : ['text'];
}
function modelReady(model: MediaModel, pixels: boolean, files = attachments.value) {
  const provider = configuration.value?.providers.find(item => item.id === model.providerId);
  if (!provider?.enabled || !provider.allowMedia || provider.kind !== 'llm' || model.capabilities.some(cap => cap.startsWith('embed_'))) return false;
  return chatCapabilities(pixels, files).every(cap => model.capabilities.includes(cap));
}
const readyModels = computed(() => configuration.value?.models.filter(model => modelReady(model, true)) || []);
const selectedModel = computed(() => configuration.value?.models.find(model => model.id === selectedModelId.value));
const reasoningLevels = computed(() => [...new Set(selectedModel.value?.reasoning?.levels || [])]);
const defaultReasoningEffort = computed(() => selectedModel.value?.reasoning?.defaultEffort || null);
function reasoningLabel(level: string) { const key = `media_chat.reasoning_level_${level}`; return te(key) ? t(key) : level; }
const reasoningDefaultLabel = computed(() => defaultReasoningEffort.value ? t('media_chat.reasoning_model_default', { level: reasoningLabel(defaultReasoningEffort.value) }) : t('media_chat.reasoning_auto'));
function normalizeReasoningChoice() { if (reasoningEffort.value === '' && !defaultReasoningEffort.value || reasoningEffort.value !== null && reasoningEffort.value !== '' && !reasoningLevels.value.includes(reasoningEffort.value)) reasoningEffort.value = null; }
const canGenerateSuggestions = computed(() => toolsEnabled.value && !loading.value && !sending.value && !activeRequestId.value && !!view.value && activeLibrary.value === libConfig._libraryId && readyModels.value.some(model => model.id === selectedModelId.value));
const canSend = computed(() => !loading.value && !sending.value && !activeRequestId.value && !!view.value && !!draft.value.trim() && draft.value.length <= 8000 && attachments.value.length <= 16 && readyModels.value.some(model => model.id === selectedModelId.value) && activeLibrary.value === libConfig._libraryId);
const modelName = (model: MediaModel) => `${configuration.value?.providers.find(provider => provider.id === model.providerId)?.name || ''} / ${model.name || model.remoteId}`;
function key() { return `${activeLibrary.value}:${view.value?.session.id || ''}`; }
function rememberDraft() { if (view.value) drafts.set(key(), { text: draft.value, attachments: [...attachments.value], modelId: selectedModelId.value, reasoningEffort: reasoningEffort.value, toolsEnabled: toolsEnabled.value, toolScope: toolScope.value, contextFields: [...contextFields.value], revision: draftRevision }); }
function restoreDraft() {
  const saved = drafts.get(key());
  draft.value = saved?.text || ''; attachments.value = saved?.attachments || []; visual.value = true;
  selectedModelId.value = saved?.modelId || view.value?.session.modelId || '';
  if (!selectedModelId.value || !readyModels.value.some(model => model.id === selectedModelId.value)) selectedModelId.value = readyModels.value.find(model => model.id === configuration.value?.roles[attachments.value.length ? 'media' : 'text'])?.id || readyModels.value[0]?.id || '';
  reasoningEffort.value = saved?.reasoningEffort ?? null; normalizeReasoningChoice();
  toolScope.value = saved?.toolScope ?? null;
  toolsEnabled.value = saved?.toolsEnabled ?? true;
  contextFields.value = normalizeChatContextFields(saved?.contextFields);
  closePicker(); actionsOpen.value = false; contextOpen.value = false; cancelRename(); sessionActionsOpen.value = false; historyMenuId.value = '';
}
function mediaSummary(file: ChatCandidate | ChatReference) { if ('source' in file) return file.fileType === 2 ? `${mediaTime(file.start)}–${mediaTime(file.end)}` : `${file.source.width} × ${file.source.height}`; return file.fileType === 2 ? mediaTime(file.duration) : `${file.width} × ${file.height}`; }
const toolNames = ['search_media', 'get_media_info', 'get_media_content', 'start_media_task', 'get_media_task', 'propose_metadata'];
function toolLabel(name: string) { return toolNames.includes(name) ? t('media_chat.tool_' + name) : name; }
function formatToolJson(value: unknown) { return JSON.stringify(value, null, 2); }
function toolObject(value: unknown): Record<string, any> | null { return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, any> : null; }
function toolTaskSnapshot(call: ChatToolCall): ToolTaskSummary | null {
  const task = toolObject(toolObject(call.localData)?.task ?? toolObject(call.result)?.task);
  return task && typeof task.id === 'string' && typeof task.state === 'string' ? task as ToolTaskSummary : null;
}
function toolTask(call: ChatToolCall): ToolTaskSummary | null {
  const snapshot = toolTaskSnapshot(call);
  if (!snapshot) return null;
  const latest = currentToolTasks.value.get(snapshot.id), state = toolTaskStates.value[snapshot.id];
  return latest ? state?.callId === latest.call.callId ? state.task : latest.task : snapshot;
}
function toolTaskUnrefreshed(call: ChatToolCall) {
  const task = toolTaskSnapshot(call), latest = task && currentToolTasks.value.get(task.id);
  return !!(task && latest && toolTaskStates.value[task.id]?.callId === latest.call.callId && toolTaskStates.value[task.id].missing);
}
async function refreshToolTasks(changedTaskId?: string) {
  const libraryId = activeLibrary.value, sessionId = view.value?.session.id;
  if (disposed || !mediaChat.open || !sessionId || libraryId !== libConfig._libraryId || toolTaskLoading?.libraryId === libraryId && toolTaskLoading.sessionId === sessionId) return;
  const tasks = [...currentToolTasks.value.values()].filter(({ call, task }) => task.id === changedTaskId || ['queued', 'pending', 'preparing', 'waiting', 'running'].includes(toolTask(call)?.state || ''));
  if (!tasks.length) return;
  const epoch = ++toolTaskReadGeneration;
  toolTaskLoading = { epoch, libraryId, sessionId };
  const current = () => !disposed && mediaChat.open && epoch === toolTaskReadGeneration && libraryId === activeLibrary.value && libraryId === libConfig._libraryId && sessionId === view.value?.session.id;
  try {
    const jobs = await invoke<ToolTaskSummary[]>('media_ai_jobs', { libraryId });
    if (!current()) return;
    for (const { call, task } of tasks) {
      if (currentToolTasks.value.get(task.id)?.call.callId !== call.callId) continue;
      const next = jobs.find(job => job.id === task.id);
      toolTaskStates.value[task.id] = { callId: call.callId, task: next || toolTask(call) || task, missing: !next };
    }
  } catch {
    if (current()) for (const { call, task } of tasks) toolTaskStates.value[task.id] = { callId: call.callId, task: toolTask(call) || task, missing: true };
  } finally { if (toolTaskLoading?.epoch === epoch) toolTaskLoading = null; }
}
interface ToolProposalFile extends Partial<ChatCandidate> { fileId: number; source: ChatSource; analysisId: string | null; proposal: { title: string; description: string; tags: string[] } }
interface ToolProposal { id: string; fields: Record<AdoptionField, boolean>; files: ToolProposalFile[] }
function toolProposal(call: ChatToolCall): ToolProposal | null {
  const proposal = toolObject(toolObject(call.localData)?.proposal);
  if (call.status !== 'completed' || !proposal || typeof proposal.id !== 'string' || !Array.isArray(proposal.files) || !toolObject(proposal.fields)) return null;
  if (!proposal.files.length || !proposal.files.every((file: any) => Number.isSafeInteger(file.fileId) && file.fileId > 0 && file.source?.fileId === file.fileId && (file.analysisId === null || typeof file.analysisId === 'string' && !!file.analysisId) && toolObject(file.proposal) && typeof file.proposal.title === 'string' && typeof file.proposal.description === 'string' && Array.isArray(file.proposal.tags))) return null;
  return proposal as unknown as ToolProposal;
}
function messageProposals(message: ChatMessage): ChatToolCall[] {
  if (message.role !== 'assistant') return [];
  const seen = new Set<string>();
  return (message.toolCalls || []).filter(call => {
    const proposal = toolProposal(call);
    if (!proposal || seen.has(proposal.id)) return false;
    seen.add(proposal.id);
    return true;
  });
}
function taskStateLabel(state: string) { const key = 'media_chat.task_state_' + state; return te(key) ? t(key) : state; }
function toolResultSummary(call: ChatToolCall) {
  if (call.status === 'running') return t('media_chat.tool_status_running');
  const result = toolObject(call.result), task = toolTask(call), proposal = toolProposal(call);
  if (result?.status && ['ambiguous', 'index_missing', 'denied', 'analysis_missing', 'proposal_missing'].includes(result.status)) return t('media_chat.tool_result_' + result.status);
  if (task) return t('media_chat.tool_task_progress', { completed: task.completed || 0, total: task.total || 0 });
  if (proposal) return t('media_chat.tool_proposal_count', { count: proposal.files.length });
  if (Array.isArray(result?.items)) return t('media_chat.tool_result_items', { count: result.items.length });
  return t('media_chat.tool_status_' + call.status);
}
async function openFile(file: ChatCandidate | ChatReference) { const libraryId = activeLibrary.value; try { if (libraryId !== libConfig._libraryId) return; if (await openMediaChatMedia(libraryId, file.fileId, 'start' in file && file.fileType === 2 ? file.start : undefined)) closeMediaChat(); } catch (reason) { if (libraryId === activeLibrary.value) error.value = String(reason); } }
async function refreshSessions(libraryId = activeLibrary.value) {
  const epoch = ++sessionListGeneration;
  const rows = await invoke<ChatSession[]>('media_chat_sessions', { libraryId });
  if (!disposed && epoch === sessionListGeneration && libraryId === activeLibrary.value && libraryId === libConfig._libraryId) sessions.value = rows;
  return rows;
}
function snapshotView(next: ChatSessionView): ChatSessionView {
  const turn = streamTurn.value;
  if (!turn || turn.libraryId !== next.session.libraryId || turn.sessionId !== next.session.id) return next;
  const answer = next.messages.find(message => message.requestId === turn.requestId && message.role === 'assistant');
  if (turn.finalView && (!answer || answer.status === 'pending')) return turn.finalView;
  if (answer && answer.status !== 'pending') { turn.closed = true; turn.finalView = next; }
  return next;
}
async function selectSession(sessionId: string) {
  rememberDraft(); historyMenuId.value = ''; sessionActionsOpen.value = false; cancelRename(); const libraryId = activeLibrary.value, epoch = ++generation; loading.value = true; error.value = '';
  try { const next = await invoke<ChatSessionView>('media_chat_inspect', { libraryId, sessionId }); if (!disposed && epoch === generation && libraryId === activeLibrary.value && libraryId === libConfig._libraryId) { messageGeneration++; view.value = snapshotView(next); restoreDraft(); showHistory.value = false; await scrollMessages(true); } }
  catch (reason) { if (!disposed && epoch === generation && libraryId === activeLibrary.value) error.value = String(reason); }
  finally { if (epoch === generation) loading.value = false; }
}
async function newSession(files: ChatCandidate[] = []) {
  if (loading.value) return; rememberDraft(); historyMenuId.value = ''; sessionActionsOpen.value = false; cancelRename(); const libraryId = activeLibrary.value, epoch = ++generation; loading.value = true; error.value = '';
  try { const next = await invoke<ChatSessionView>('media_chat_create', { libraryId, title: null }); if (disposed || epoch !== generation || libraryId !== activeLibrary.value || libraryId !== libConfig._libraryId) return; messageGeneration++; view.value = next; restoreDraft(); showHistory.value = false; incoming.value = []; incomingIds.value = []; await addCandidates(files); await refreshSessions(libraryId); await scrollMessages(true); }
  catch (reason) { if (epoch === generation) error.value = String(reason); }
  finally { if (epoch === generation) loading.value = false; }
}
async function openEntry() {
  const libraryId = mediaChat.libraryId; if (!libraryId || libraryId !== libConfig._libraryId) return;
  const epoch = ++generation; loading.value = true; error.value = ''; notice.value = '';
  try {
    if (activeLibrary.value !== libraryId) { rememberDraft(); activeLibrary.value = libraryId; view.value = null; attachments.value = []; draft.value = ''; incoming.value = []; incomingIds.value = []; thumbnails.value = {}; }
    const [config, rows] = await Promise.all([invoke<{ configuration: MediaConfiguration }>('media_ai_configuration'), invoke<ChatSession[]>('media_chat_sessions', { libraryId })]);
    if (disposed || epoch !== generation || libraryId !== mediaChat.libraryId) return;
    configuration.value = config.configuration; sessions.value = rows;
    if (!view.value) { const restored = rows[0] ? await invoke<ChatSessionView>('media_chat_inspect', { libraryId, sessionId: rows[0].id }) : await invoke<ChatSessionView>('media_chat_create', { libraryId, title: null }); if (disposed || epoch !== generation) return; view.value = snapshotView(restored); restoreDraft(); await refreshSessions(libraryId); }
    if (mediaChat.fileIds.length) {
      const ids = validMediaChatIds(mediaChat.fileIds);
      if (ids.length > 200) { incomingIds.value = ids; incoming.value = []; notice.value = t('media_chat.candidate_limit'); return; }
      const files = await resolveIds(ids);
      if (disposed || epoch !== generation) return;
      if (!view.value.messages.length && !draft.value && !attachments.value.length && ids.length <= 16) await addCandidates(files);
      else { incoming.value = files; incomingIds.value = ids; }
    }
    await scrollMessages(true);
  } catch (reason) { if (epoch === generation) error.value = String(reason); }
  finally { if (epoch === generation) loading.value = false; }
}
function cancelRename() { renaming.value = false; renameTarget.value = null; }
function beginRename(session = view.value?.session) { if (!session) return; renameTarget.value = { libraryId: activeLibrary.value, sessionId: session.id }; renameText.value = session.title; renaming.value = true; sessionActionsOpen.value = false; historyMenuId.value = ''; void nextTick(() => { renameInput.value?.focus(); renameInput.value?.select(); }); }
function toggleHistoryMenu(event: MouseEvent, sessionId: string) { if (historyMenuId.value === sessionId) { historyMenuId.value = ''; return; } const bounds = (event.currentTarget as HTMLElement).getBoundingClientRect(); historyMenuStyle.value = { left: `${Math.max(frame.value.x + 8, Math.min(bounds.right - 160, frame.value.x + frame.value.width - 168))}px`, top: `${Math.max(frame.value.y + 52, Math.min(bounds.bottom + 4, frame.value.y + frame.value.height - 100))}px` }; historyMenuId.value = sessionId; }
function toggleHistory() { showHistory.value = !showHistory.value; sessionActionsOpen.value = false; historyMenuId.value = ''; cancelRename(); closePicker(); actionsOpen.value = false; const libraryId = activeLibrary.value; if (showHistory.value) void refreshSessions(libraryId).catch(reason => { if (!disposed && libraryId === activeLibrary.value && libraryId === libConfig._libraryId) error.value = String(reason); }); else void scrollMessages(true); }
async function renameSession() {
  const target = renameTarget.value || (view.value ? { libraryId: activeLibrary.value, sessionId: view.value.session.id } : null);
  if (!target || !renameText.value.trim() || renamingBusy.value || target.libraryId !== activeLibrary.value || target.libraryId !== libConfig._libraryId) return;
  const { libraryId, sessionId } = target, title = renameText.value.trim(); renamingBusy.value = true;
  try { const renamed = await invoke<ChatSession>('media_chat_rename', { libraryId, sessionId, title }); if (disposed || libraryId !== activeLibrary.value || libraryId !== libConfig._libraryId) return; if (view.value?.session.id === sessionId) view.value.session = renamed; if (renameTarget.value?.sessionId === sessionId || !renameTarget.value) cancelRename(); await refreshSessions(libraryId); }
  catch (reason) { if (!disposed && libraryId === activeLibrary.value && libraryId === libConfig._libraryId) error.value = String(reason); }
  finally { renamingBusy.value = false; }
}
async function deleteSession(session = view.value?.session) {
  if (!session || session.runningRequestId || request.value?.libraryId === activeLibrary.value && request.value?.sessionId === session.id) return;
  const libraryId = activeLibrary.value, sessionId = session.id, fromHistory = showHistory.value; sessionActionsOpen.value = false; historyMenuId.value = '';
  if (!await ask(t('media_chat.delete_confirm', { name: session.title }), { kind: 'warning' }) || disposed || libraryId !== activeLibrary.value || libraryId !== libConfig._libraryId) return;
  if (request.value?.libraryId === libraryId && request.value?.sessionId === sessionId || sessions.value.some(item => item.id === sessionId && item.runningRequestId)) return;
  try { await invoke('media_chat_delete', { libraryId, sessionId }); drafts.delete(`${libraryId}:${sessionId}`); if (disposed || activeLibrary.value !== libraryId || libraryId !== libConfig._libraryId) return; const rows = await refreshSessions(libraryId); if (view.value?.session.id !== sessionId) return; messageGeneration++; view.value = null; if (rows[0]) await selectSession(rows[0].id); else await newSession(); if (!disposed && fromHistory && libraryId === activeLibrary.value && libraryId === libConfig._libraryId) showHistory.value = true; }
  catch (reason) { if (!disposed && libraryId === activeLibrary.value && libraryId === libConfig._libraryId) error.value = String(reason); }
}
async function resolveIds(ids: number[]) {
  if (ids.length > 16) notice.value = t('media_chat.reference_limit');
  const validIds = validMediaChatIds(ids);
  if (validIds.length > 200) throw new Error(t('media_chat.candidate_limit'));
  return invoke<ChatCandidate[]>('media_chat_media', { libraryId: activeLibrary.value, query: null, fileIds: validIds, limit: 200 });
}
async function addCandidates(files: ChatCandidate[]) {
  const all = new Map(attachments.value.map(file => [file.fileId, file])); for (const file of files) all.set(file.fileId, file);
  if (all.size > 16) { notice.value = t('media_chat.reference_limit'); return false; }
  attachments.value = [...all.values()]; rememberDraft(); void loadThumbnails(files); return true;
}
async function attachIds(ids: number[]) {
  const libraryId = activeLibrary.value, sessionId = view.value?.session.id;
  try {
    const validIds = validMediaChatIds(ids), files = validIds.length <= 200 ? await resolveIds(validIds) : [];
    if (disposed || libraryId !== activeLibrary.value || libraryId !== libConfig._libraryId || sessionId !== view.value?.session.id) return;
    if (validIds.length > 16) { incomingIds.value = validIds; incoming.value = files; notice.value = t(validIds.length > 200 ? 'media_chat.candidate_limit' : 'media_chat.reference_limit'); return; }
    if (files.length < validIds.length) notice.value = t('media_chat.media_unavailable');
    await addCandidates(files);
  }
  catch (reason) { if (!disposed && libraryId === activeLibrary.value && libraryId === libConfig._libraryId && sessionId === view.value?.session.id) error.value = String(reason); }
}
async function addIncoming() { if (incomingIds.value.length > 16) { notice.value = t('media_chat.reference_limit'); return; } if (await addCandidates(incoming.value)) { incoming.value = []; incomingIds.value = []; } }
function removeAttachment(fileId: number) { attachments.value = attachments.value.filter(file => file.fileId !== fileId); rememberDraft(); }
async function loadThumbnails(files: ChatCandidate[]) {
  const libraryId = activeLibrary.value;
  await Promise.all(files.slice(0, 60).filter(file => !thumbnails.value[file.fileId]).map(async file => { try { const url = isWin ? getThumbnailDataUrl(await getFileThumbById(file.fileId, 128, false), '', false, 128) : getThumbUrl(file.fileId, false, 128); if (!disposed && libraryId === activeLibrary.value && url) thumbnails.value[file.fileId] = url; } catch { /* The name remains usable without a thumbnail. */ } }));
}
function closePicker() { mention.value = null; pickerOpen.value = false; candidateGeneration++; }
function openPicker() { actionsOpen.value = false; contextOpen.value = false; mention.value = null; pickerOpen.value = true; pickerQuery.value = ''; candidateSource.value = sessionMedia.value.length ? 'session' : 'library'; void loadCandidates(); void nextTick(() => pickerSearch.value?.focus()); }
function updateMention() { const caret = composer.value?.selectionStart ?? draft.value.length; mention.value = mentionAtCaret(draft.value, caret); if (mention.value) { pickerOpen.value = false; actionsOpen.value = false; contextOpen.value = false; } else { const slash = slashAtCaret(draft.value, caret); if (slash !== null) { closePicker(); contextOpen.value = false; actionsOpen.value = true; actionSlash = slash; actionIndex.value = 0; } else if (actionSlash !== null) { actionsOpen.value = false; actionSlash = null; } } }
function composerKeyup(event: KeyboardEvent) { if (!['Escape', 'Enter', 'Tab', 'ArrowUp', 'ArrowDown'].includes(event.key)) updateMention(); }
async function loadCandidates() {
  const epoch = ++candidateGeneration, libraryId = activeLibrary.value, query = mention.value?.query || pickerQuery.value;
  if (!mention.value && !pickerOpen.value) return;
  candidateLoading.value = true;
  try { const rows = candidateSource.value === 'session' ? sessionMedia.value.filter(file => `${file.name} ${file.folderPath}`.toLowerCase().includes(query.toLowerCase())).slice(0, 60) : await invoke<ChatCandidate[]>('media_chat_media', { libraryId, query, fileIds: null, limit: 60 }); if (!disposed && epoch === candidateGeneration && libraryId === activeLibrary.value) { candidates.value = rows; candidateIndex.value = 0; void loadThumbnails(rows); } }
  catch (reason) { if (epoch === candidateGeneration) error.value = String(reason); }
  finally { if (epoch === candidateGeneration) candidateLoading.value = false; }
}
async function pickCandidate(file: ChatCandidate) {
  if (picking) return;
  picking = true;
  const range = mention.value, originalDraft = draft.value, sessionId = view.value?.session.id, libraryId = activeLibrary.value;
  try { const current = (await resolveIds([file.fileId]))[0]; if (libraryId !== activeLibrary.value || sessionId !== view.value?.session.id) return; if (!current) { notice.value = t('media_chat.media_unavailable'); return; } if (!await addCandidates([current])) return; if (range && draft.value === originalDraft) { const inserted = insertMediaMention(draft.value, range, current.name); draft.value = inserted.text; await nextTick(); composer.value?.setSelectionRange(inserted.caret, inserted.caret); } closePicker(); composer.value?.focus(); rememberDraft(); }
  catch (reason) { error.value = String(reason); }
  finally { picking = false; }
}
function candidateKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); closePicker(); return true; }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); if (candidates.value.length) candidateIndex.value = (candidateIndex.value + (event.key === 'ArrowDown' ? 1 : -1) + candidates.value.length) % candidates.value.length; return true; }
  if (event.key === 'Enter' || event.key === 'Tab') { event.preventDefault(); if (candidates.value[candidateIndex.value]) void pickCandidate(candidates.value[candidateIndex.value]); return true; }
  return false;
}
function composerKeydown(event: KeyboardEvent) { if (event.isComposing) return; if ((mention.value || pickerOpen.value) && candidateKeydown(event)) return; if (actionsOpen.value && actionKeydown(event)) return; if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); void send(); } }
function dialogKeydown(event: KeyboardEvent) { if (event.key === 'Escape' && uiStore.isInputActive('MediaChat')) { event.preventDefault(); if (previewOpen.value) previewOpen.value = false; else if (renaming.value) cancelRename(); else if (mention.value || pickerOpen.value) closePicker(); else if (contextOpen.value) contextOpen.value = false; else if (actionsOpen.value) { actionsOpen.value = false; actionSlash = null; } else if (sessionActionsOpen.value || historyMenuId.value) { sessionActionsOpen.value = false; historyMenuId.value = ''; } else if (showHistory.value) toggleHistory(); else closeMediaChat(); } }
async function send() {
  if (!canSend.value || !view.value) return;
  const libraryId = activeLibrary.value, sessionId = view.value.session.id, question = draft.value, refs = attachments.value.map(file => ({ fileId: file.fileId, start: null })), modelId = selectedModelId.value, effort = reasoningEffort.value, requestId = crypto.randomUUID();
  const scope = toolScope.value, fields = [...contextFields.value], useTools = toolsEnabled.value;
  const createdAt = Date.now(), media = attachments.value.map(file => ({ ...file }));
  const base = { modelId, modelName: selectedModel.value?.name || selectedModel.value?.remoteId || '', visual: refs.length > 0, reasoningEffort: effort, references: [], error: null, requestId, createdAt, reasoningContent: '', toolCalls: [] };
  rememberDraft(); request.value = { id: requestId, sessionId, libraryId }; error.value = ''; closePicker(); actionsOpen.value = false; contextOpen.value = false;
  streamTurn.value = { libraryId, sessionId, requestId, seq: 0, closed: false, cancelRequested: false, registeredCancelSent: false, media,
    user: { ...base, id: `local-user-${requestId}`, role: 'user', status: 'completed', phase: 'completed', content: question },
    assistant: { ...base, id: `local-assistant-${requestId}`, role: 'assistant', status: 'pending', phase: 'preparing', content: '' },
  };
  messageGeneration++; draft.value = ''; rememberDraft(); const clearedRevision = drafts.get(`${libraryId}:${sessionId}`)?.revision;
  const restoreQuestion = () => { const saved = drafts.get(`${libraryId}:${sessionId}`); if (!saved || saved.revision !== clearedRevision) return; saved.text = question; if (!disposed && libraryId === activeLibrary.value && sessionId === view.value?.session.id) { draft.value = question; rememberDraft(); } };
  void scrollMessages(true);
  const channel = new Channel<ChatStreamEvent>(); channel.onmessage = receiveStreamEvent;
  try {
    const next = await invoke<ChatSessionView>('media_chat_send', { libraryId, sessionId, modelId, question, references: refs, visual: refs.length > 0, reasoningEffort: effort, toolsEnabled: useTools, scope, contextFields: fields, requestId, onEvent: channel });
    if (disposed) return;
    const turn = streamTurn.value;
    if (turn?.requestId === requestId && turn.libraryId === libraryId && turn.sessionId === sessionId) { turn.closed = true; turn.finalView = next; }
    if (!next.messages.some(message => message.requestId === requestId)) restoreQuestion();
    messageGeneration++;
    if (libraryId === activeLibrary.value && sessionId === view.value?.session.id) {
      view.value = snapshotView(next); await scrollMessages();
      await autoReviewToolProposal(next.messages.find(message => message.requestId === requestId && message.role === 'assistant'), libraryId, sessionId);
    }
  } catch (reason) {
    const turn = streamTurn.value;
    if (turn?.requestId === requestId && !turn.finalView) { turn.closed = true; turn.assistant.status = 'failed'; turn.assistant.phase = 'failed'; turn.assistant.error = String(reason); }
    restoreQuestion(); messageGeneration++;
    if (!disposed && libraryId === activeLibrary.value && sessionId === view.value?.session.id) error.value = String(reason);
  } finally {
    if (request.value?.id === requestId) request.value = null;
    if (!disposed && libraryId === activeLibrary.value && libraryId === libConfig._libraryId) void refreshSessions(libraryId).catch(reason => { if (!disposed && libraryId === activeLibrary.value && libraryId === libConfig._libraryId) error.value = String(reason); });
  }
}
function receiveStreamEvent(event: ChatStreamEvent) {
  const turn = streamTurn.value;
  if (disposed || !turn || turn.closed || event.libraryId !== turn.libraryId || event.sessionId !== turn.sessionId || event.requestId !== turn.requestId || !Number.isSafeInteger(event.seq) || event.seq <= turn.seq) return;
  turn.seq = event.seq; turn.assistant.phase = event.phase; messageGeneration++;
  if (event.kind === 'text_delta' && event.delta) turn.assistant.content += event.delta;
  else if (event.kind === 'reasoning_delta' && event.delta) turn.assistant.reasoningContent += event.delta;
  else if (event.kind === 'tool' && event.toolCall) {
    const calls = [...(turn.assistant.toolCalls || [])], index = calls.findIndex(call => call.callId === event.toolCall!.callId);
    if (index < 0) calls.push(event.toolCall); else calls[index] = event.toolCall;
    turn.assistant.toolCalls = calls;
  }
  if (event.phase === 'completed' || event.phase === 'failed' || event.phase === 'cancelled') { turn.closed = true; turn.assistant.status = event.phase; }
  // The first valid frame confirms registration; an earlier cancellation may have been a no-op.
  if (turn.cancelRequested && !turn.registeredCancelSent && !turn.closed) {
    turn.registeredCancelSent = true;
    void invoke('media_ai_cancel_request', { requestId: turn.requestId }).catch(reason => { if (!disposed && turn.libraryId === activeLibrary.value && turn.libraryId === libConfig._libraryId && turn.sessionId === view.value?.session.id) error.value = String(reason); });
  }
}
async function stopAnswer() {
  const requestId = activeRequestId.value, libraryId = activeLibrary.value, sessionId = view.value?.session.id; if (!requestId) return;
  const turn = streamTurn.value;
  if (turn?.requestId === requestId && turn.libraryId === libraryId && turn.sessionId === sessionId && !turn.closed) { turn.cancelRequested = true; if (turn.seq > 0) turn.registeredCancelSent = true; }
  try { await invoke('media_ai_cancel_request', { requestId }); if (!disposed && libraryId === activeLibrary.value && sessionId === view.value?.session.id) await refreshCurrent(); } catch (reason) { if (!disposed && libraryId === activeLibrary.value && sessionId === view.value?.session.id) error.value = String(reason); }
}
async function refreshCurrent() {
  const libraryId = activeLibrary.value, sessionId = view.value?.session.id, epoch = ++messageGeneration; if (!sessionId) return;
  const next = await invoke<ChatSessionView>('media_chat_inspect', { libraryId, sessionId }); if (!disposed && epoch === messageGeneration && libraryId === activeLibrary.value && libraryId === libConfig._libraryId && sessionId === view.value?.session.id) view.value = snapshotView(next);
}
function messagesScrolled() { if (!messagesRoot.value) return; scrollRevision++; nearBottom.value = isChatNearBottom(messagesRoot.value); if (nearBottom.value) newContent.value = false; }
function markdownRendered() { if (mediaChat.open && !showHistory.value) return scrollMessages(); }
async function scrollMessages(force = false) { const follow = force || nearBottom.value, revision = scrollRevision, sessionId = view.value?.session.id, libraryId = activeLibrary.value; await nextTick(); if (sessionId !== view.value?.session.id || libraryId !== activeLibrary.value) return; if (revision !== scrollRevision && !nearBottom.value) { newContent.value = true; return; } if (follow) { if (messagesRoot.value) messagesRoot.value.scrollTop = messagesRoot.value.scrollHeight; nearBottom.value = true; newContent.value = false; } else newContent.value = true; }
async function internalDrop(event: Event) { const detail = (event as CustomEvent<{ libraryId: string; fileIds: number[] }>).detail; if (detail?.libraryId !== activeLibrary.value) { notice.value = t('media_chat.wrong_library'); return; } showHistory.value = false; historyMenuId.value = ''; await attachIds(detail.fileIds); }
async function dropFiles(event: DragEvent) {
  dragOver.value = false; showHistory.value = false; historyMenuId.value = ''; const paths = getExternalDropPaths(event.dataTransfer); const hasFiles = !!event.dataTransfer?.files.length;
  const libraryId = activeLibrary.value, sessionId = view.value?.session.id;
  try { if (!paths.length && (hasFiles || isMac)) paths.push(...((await getDragPayload())?.filePaths || [])); if (!paths.length) { notice.value = t('media_chat.import_first'); return; } const uniquePaths = [...new Set(paths)]; if (uniquePaths.length > 200) { notice.value = t('media_chat.candidate_limit'); return; } const files = await invoke<ChatCandidate[]>('media_chat_media', { libraryId, query: null, fileIds: null, paths: uniquePaths, limit: 200 }); if (libraryId !== activeLibrary.value || sessionId !== view.value?.session.id) return; if (files.length < uniquePaths.length) notice.value = t('media_chat.import_first'); await addCandidates(files); } catch (reason) { error.value = String(reason); }
}
function openTasks(all: boolean) { previewOpen.value = false; openMediaAiBatch(activeLibrary.value, all ? null : attachments.value.length ? attachments.value.map(file => file.fileId) : sessionMedia.value.map(file => file.fileId), selectedModelId.value || null); }
function openIncomingTasks() { previewOpen.value = false; openMediaAiBatch(activeLibrary.value, [...incomingIds.value], selectedModelId.value || null); }

type AdoptionField = 'title' | 'description' | 'tags';
interface PreviewRow { file: ChatCandidate; source: ChatSource | null; analyses: any[]; analysisId: string | null; ready: boolean; availableFields: AdoptionField[]; fields: Record<AdoptionField, boolean>; proposal: { title: string; description: string; tags: string[] }; tagsText: string; tagDraft: string; tagEdit: { tag: string; text: string } | null; currentTags: string[]; expandedDescription: boolean; error: string; applied: boolean }
const adoptionFields: AdoptionField[] = ['title', 'description', 'tags'];
const previewOpen = ref(false), previewRows = ref<PreviewRow[]>([]), previewError = ref(''), onlyEmpty = ref(true), applying = ref(false);
const previewTagCatalog = ref<string[] | null>(null);
const previewTagCatalogKeys = computed(() => previewTagCatalog.value && new Set(previewTagCatalog.value.map(previewTagKey)));
const previewHasText = computed(() => previewRows.value.some(row => row.availableFields.some(field => field !== 'tags')));
const previewHasTags = computed(() => previewRows.value.some(row => row.availableFields.includes('tags')));
let suggestionLibrary = '';
const reviewingToolProposal = ref(false);
const reviewedToolProposals = new Set<string>();
let toolReviewGeneration = 0;
async function autoReviewToolProposal(message: ChatMessage | undefined, libraryId: string, sessionId: string) {
  if (disposed || !mediaChat.open || showHistory.value || loading.value || previewOpen.value || applying.value || reviewingToolProposal.value
    || previewRows.value.some(row => row.ready && !row.applied)
    || libraryId !== activeLibrary.value || libraryId !== libConfig._libraryId || sessionId !== view.value?.session.id
    || message?.status !== 'completed') return;
  const call = messageProposals(message).at(-1), proposal = call && toolProposal(call);
  if (call && proposal && !reviewedToolProposals.has(`${libraryId}:${sessionId}:${proposal.id}`)) await reviewToolProposal(call);
}
async function reviewToolProposal(call: ChatToolCall) {
  const proposal = toolProposal(call);
  if (!proposal || applying.value || reviewingToolProposal.value || !view.value) return;
  const libraryId = activeLibrary.value, sessionId = view.value.session.id, epoch = ++toolReviewGeneration;
  reviewedToolProposals.add(`${libraryId}:${sessionId}:${proposal.id}`);
  const current = () => !disposed && epoch === toolReviewGeneration && libraryId === activeLibrary.value && libraryId === libConfig._libraryId && sessionId === view.value?.session.id;
  suggestionLibrary = libraryId;
  previewError.value = '';
  previewRows.value = [];
  previewTagCatalog.value = null;
  previewOpen.value = true;
  reviewingToolProposal.value = true;
  for (const item of proposal.files) {
    const source: ChatSource = JSON.parse(JSON.stringify(item.source));
    const file: ChatCandidate = {
      fileId: item.fileId,
      name: item.name || source.path.split(/[\\/]/).pop() || String(item.fileId),
      folderPath: source.path.replace(/[\\/][^\\/]*$/, ''),
      fileType: item.fileType || source.kind,
      width: item.width ?? source.width,
      height: item.height ?? source.height,
      duration: item.duration ?? source.duration,
      cloudOnly: source.cloudOnly,
      hasAnalysis: !!item.analysisId,
      sourceStamp: item.sourceStamp || '',
    };
    previewRows.value.push({ file, source, analyses: [], analysisId: item.analysisId, ready: false, availableFields: adoptionFields.filter(field => proposal.fields[field]), fields: { title: !!proposal.fields.title, description: !!proposal.fields.description, tags: !!proposal.fields.tags }, proposal: JSON.parse(JSON.stringify(item.proposal)), tagsText: item.proposal.tags.join('、'), tagDraft: '', tagEdit: null, currentTags: [], expandedDescription: false, error: '', applied: false });
  }
  void loadThumbnails(previewRows.value.map(row => row.file));
  try {
    if (previewHasTags.value) {
      const tags = await getAllTags(0).catch(() => null);
      if (!current()) return;
      previewTagCatalog.value = Array.isArray(tags) ? tags.map((tag: { name: string }) => tag.name) : null;
    }
    for (let index = 0; index < proposal.files.length; index++) {
      if (!current()) return;
      const item = proposal.files[index], row = previewRows.value[index], source = row.source!;
      try {
        const [inspected, tags] = await Promise.all([
          invoke<{ source: ChatSource; analyses: any[] }>('media_ai_inspect', { libraryId, fileId: item.fileId }),
          getTagsForFile(item.fileId),
        ]);
        if (!current()) return;
        const keys: Array<keyof ChatSource> = ['fileId', 'path', 'modified', 'size', 'orientation', 'kind', 'width', 'height', 'duration', 'title', 'comments', 'cloudOnly'];
        const sameTags = (a: number[], b: number[]) => JSON.stringify([...a].sort((x, y) => x - y)) === JSON.stringify([...b].sort((x, y) => x - y));
        if (!keys.every(key => inspected.source[key] === source[key]) || !sameTags(inspected.source.tagIds, source.tagIds) || item.analysisId !== null && !inspected.analyses.some(analysis => analysis.id === item.analysisId)) {
          row.error = t('media_chat.tool_proposal_stale');
          continue;
        }
        row.analyses = inspected.analyses;
        row.analysisId = item.analysisId;
        row.fields = previewFields(row);
        row.currentTags = (tags || []).map((tag: any) => tag.name);
        row.ready = true;
      } catch (reason) {
        if (current()) row.error = String(reason);
      }
    }
  } finally { if (epoch === toolReviewGeneration) reviewingToolProposal.value = false; }
}
function previewAnalysis(row: PreviewRow) { return row.analysisId ? row.analyses.find(item => item.id === row.analysisId) : null; }
function previewDismissed(row: PreviewRow) { return previewAnalysis(row)?.dismissed || []; }
function previewVisibleFields(row: PreviewRow) { return row.availableFields; }
function previewFields(row: PreviewRow) { const allowed = (field: AdoptionField) => row.availableFields.includes(field) && row.fields[field] && !previewDismissed(row).includes(field); return { title: allowed('title'), description: allowed('description'), tags: allowed('tags') }; }
function previewTagKey(tag: string) { return tag.trim().replace(/[A-Z]/g, character => character.toLowerCase()); }
function parsePreviewTags(text: string) {
  const seen = new Set<string>();
  return text.split(/[、,，;；\n]/).map(tag => tag.trim()).filter(tag => { const key = previewTagKey(tag); if (!key || seen.has(key)) return false; seen.add(key); return true; });
}
function previewTags(row: PreviewRow) { return parsePreviewTags(row.tagsText); }
function previewTagEditValue(row: PreviewRow, tag: string) { return row.tagEdit?.tag === tag ? row.tagEdit.text : tag; }
function previewTagInputWidth(tag: string) { return Math.max(3, Math.min(28, Array.from(tag).reduce((width, character) => width + (character.codePointAt(0)! > 255 ? 2 : 1), 2))); }
function pendingPreviewTags(row: PreviewRow) { return parsePreviewTags([...previewTags(row).map(tag => previewTagEditValue(row, tag)), row.tagDraft || ''].join('、')); }
function canEditPreviewTags(row: PreviewRow) { return !applying.value && row.ready && !row.applied && row.availableFields.includes('tags') && !previewDismissed(row).includes('tags'); }
function editPreviewTag(row: PreviewRow, tag: string, text: string) {
  if (!canEditPreviewTags(row)) return;
  if (row.tagEdit && row.tagEdit.tag !== tag) addPreviewTags(row);
  row.tagEdit = { tag, text };
}
function addPreviewTags(row: PreviewRow) {
  if (!canEditPreviewTags(row)) return;
  row.tagsText = pendingPreviewTags(row).join('、'); row.tagDraft = ''; row.tagEdit = null;
}
function removePreviewTag(row: PreviewRow, tag: string) {
  if (!canEditPreviewTags(row)) return;
  row.tagsText = previewTags(row).filter(value => previewTagKey(value) !== previewTagKey(tag)).join('、');
  if (row.tagEdit?.tag === tag) row.tagEdit = null;
}
function previewTagKeydown(row: PreviewRow, event: KeyboardEvent) {
  if (!event.isComposing && ['Enter', ',', '，', ';', '；', '、'].includes(event.key)) { event.preventDefault(); addPreviewTags(row); }
}
function previewTagEditKeydown(row: PreviewRow, event: KeyboardEvent) {
  if (event.isComposing || !canEditPreviewTags(row)) return;
  if (event.key === 'Enter') { event.preventDefault(); addPreviewTags(row); (event.target as HTMLInputElement | null)?.blur(); }
  else if (event.key === 'Escape' && row.tagEdit) { event.preventDefault(); event.stopPropagation(); row.tagEdit = null; }
}
function previewTagState(row: PreviewRow, tag: string): 'existing' | 'add' | 'new' | 'unknown' {
  const key = previewTagKey(tag);
  if (row.currentTags.some(value => previewTagKey(value) === key)) return 'existing';
  if (previewTagCatalogKeys.value === null) return 'unknown';
  return previewTagCatalogKeys.value.has(key) ? 'add' : 'new';
}
function previewTagValidation(row: PreviewRow) {
  if (!previewFields(row).tags) return '';
  const tags = pendingPreviewTags(row);
  return tags.length > 20 || tags.some(tag => Array.from(tag).length > 64 || /[\u0000-\u001f\u007f-\u009f]/.test(tag)) ? t('media_chat.preview_invalid_tags') : '';
}
function previewStatus(row: PreviewRow) { return t(row.applied ? 'media_chat.applied' : row.error ? 'media_chat.preview_failed' : row.ready ? 'media_chat.preview_pending' : 'media_chat.preparing_suggestions'); }
function previewCanApply(row: PreviewRow) { return row.ready && !!row.source && !row.error && !row.applied && !previewTagValidation(row) && (row.analysisId === null || !!previewAnalysis(row)) && Object.values(previewFields(row)).some(Boolean); }
const adoptionCount = computed(() => previewRows.value.filter(previewCanApply).length);
const previewSelectedFieldCount = computed(() => previewRows.value.filter(previewCanApply).reduce((count, row) => count + Object.values(previewFields(row)).filter(Boolean).length, 0));
type MenuAction = 'attach' | 'title' | 'description' | 'tags' | 'review' | 'selected' | 'tasks' | 'all' | 'configure';
const menuItems = computed(() => {
  const unavailable = !attachments.value.length || !canGenerateSuggestions.value || applying.value || reviewingToolProposal.value;
  const items: Array<{ id: MenuAction; label: string; disabled: boolean }> = [
    { id: 'attach', label: 'attach', disabled: false },
    { id: 'title', label: 'fill_title', disabled: unavailable },
    { id: 'description', label: 'fill_description', disabled: unavailable },
    { id: 'tags', label: 'fill_tags', disabled: unavailable },
  ];
  if (previewRows.value.length) items.push({ id: 'review', label: 'review_suggestions', disabled: false });
  if (incomingIds.value.length) items.push({ id: 'selected', label: 'selected_media_tasks', disabled: false });
  items.push({ id: 'tasks', label: 'tasks', disabled: false }, { id: 'all', label: 'all_media_tasks', disabled: false }, { id: 'configure', label: 'configure', disabled: false });
  return items;
});
function toggleContext() { const opening = !contextOpen.value; closePicker(); actionsOpen.value = false; actionSlash = null; contextOpen.value = opening; }
function toggleActions() { const opening = !actionsOpen.value; closePicker(); contextOpen.value = false; actionSlash = null; actionsOpen.value = opening; actionIndex.value = 0; }
function dismissActionsOutside(event: PointerEvent) { const target = event.target as HTMLElement; if (!target?.closest('[data-chat-context]')) contextOpen.value = false; if (!target?.closest('[data-chat-actions]')) { actionsOpen.value = false; actionSlash = null; } if (!target?.closest('[data-chat-session-menu]')) { sessionActionsOpen.value = false; historyMenuId.value = ''; } }
async function runMenuAction(id: MenuAction) {
  const item = menuItems.value.find(item => item.id === id); if (!item || item.disabled) return;
  if (actionSlash !== null && draft.value[actionSlash] === '/') draft.value = draft.value.slice(0, actionSlash) + draft.value.slice(actionSlash + 1);
  actionsOpen.value = false; actionSlash = null;
  if (id === 'attach') openPicker();
  else if (id === 'title' || id === 'description' || id === 'tags') await generateSuggestions(id);
  else if (id === 'review') previewOpen.value = true;
  else if (id === 'selected') openIncomingTasks();
  else if (id === 'tasks' || id === 'all') openTasks(id === 'all');
  else if (id === 'configure') await openMediaAiSettings();
}
function actionKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); actionsOpen.value = false; actionSlash = null; return true; }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault(); const direction = event.key === 'ArrowDown' ? 1 : -1;
    for (let i = 0; i < menuItems.value.length; i++) { actionIndex.value = (actionIndex.value + direction + menuItems.value.length) % menuItems.value.length; if (!menuItems.value[actionIndex.value].disabled) break; }
    return true;
  }
  if (event.key === 'Enter' || event.key === 'Tab') { event.preventDefault(); const item = menuItems.value[actionIndex.value]; if (item) void runMenuAction(item.id); return true; }
  return false;
}
async function generateSuggestions(field: AdoptionField) {
  if (!attachments.value.length || !canGenerateSuggestions.value || applying.value || reviewingToolProposal.value) return;
  const prompt = t('media_chat.suggestion_prompt', { field: t('media_chat.field_' + field) });
  draft.value = [draft.value.trim(), prompt].filter(Boolean).join('\n\n');
  rememberDraft();
  await send();
}
function selectPreviewAnalysis(row: PreviewRow) { const analysis = row.analyses.find(item => item.id === row.analysisId); if (!analysis) return; row.fields = previewFields(row); row.proposal = { title: analysis.title, description: analysis.description, tags: [...analysis.tags] }; row.tagsText = analysis.tags.join('、'); row.tagDraft = ''; row.tagEdit = null; }
async function applySuggestions() {
  if (applying.value || reviewingToolProposal.value || suggestionLibrary !== libConfig._libraryId) return;
  for (const row of previewRows.value) addPreviewTags(row);
  if (!adoptionCount.value) return;
  const libraryId = suggestionLibrary, sessionId = view.value?.session.id, epoch = toolReviewGeneration;
  const current = () => !disposed && epoch === toolReviewGeneration && libraryId === activeLibrary.value && libraryId === libConfig._libraryId && sessionId === view.value?.session.id;
  const payloads = () => previewRows.value.filter(previewCanApply).map(row => ({ libraryId, source: JSON.parse(JSON.stringify(row.source)), analysisId: row.analysisId, options: { ...previewFields(row), proposal: { title: row.proposal.title, description: row.proposal.description, tags: previewFields(row).tags ? pendingPreviewTags(row) : [] }, createTags: true, onlyEmpty: onlyEmpty.value } }));
  const targets = payloads(), snapshot = JSON.stringify(targets);
  if (!await pendingChanges.confirm('media-title') || !current() || snapshot !== JSON.stringify(payloads())) return;
  applying.value = true;
  try { for (const target of targets) { if (!current()) break; const row = previewRows.value.find(row => row.analysisId === target.analysisId && row.file.fileId === target.source.fileId); if (!row) break; try { await invoke('media_ai_adopt', target); row.applied = true; row.error = ''; } catch (reason) { row.error = String(reason); } } }
  finally { applying.value = false; }
}

let gesture: { mode: 'move' | 'resize'; edge?: ChatResizeEdge; pointerId: number; x: number; y: number; frame: ChatFrame; target: HTMLElement } | undefined;
function fitFrame() {
  endFrameGesture();
  if (autoDockWindow.value && !dockAnchor) dockAnchor = captureChatDockAnchor(frame.value, viewport.value);
  viewport.value = { width: window.innerWidth, height: window.innerHeight };
  frame.value = autoDockWindow.value && dockAnchor ? dockChatFrame(dockAnchor, viewport.value) : fitChatFrame(frame.value, viewport.value);
  saveFrame();
}
function saveFrame() { try { localStorage.setItem('lap.media-chat.window', JSON.stringify({ ...frame.value, ...(autoDockWindow.value && dockAnchor ? { dock: dockAnchor } : {}) })); } catch { /* The dialog remains usable without storage. */ } }
function updateDockAfterGesture(mode: 'move' | 'resize', initial: ChatFrame) {
  const previous = dockAnchor, next = captureChatDockAnchor(frame.value, viewport.value, previous);
  if (previous && mode === 'resize') {
    next.side = previous.side;
    if (frame.value.width === initial.width) next.width = previous.width;
    if (frame.value.height === initial.height) next.height = previous.height;
  }
  if (previous && mode === 'move') { next.width = previous.width; next.height = previous.height; }
  dockAnchor = next;
  frame.value = dockChatFrame(next, viewport.value);
}
function toggleAutoDockWindow() { config.setMediaChat({ autoDockWindow: !autoDockWindow.value }); sessionActionsOpen.value = false; }
watch(autoDockWindow, enabled => {
  if (!frameReady) return;
  endFrameGesture();
  dockAnchor = enabled ? captureChatDockAnchor(frame.value, viewport.value, dockAnchor) : null;
  if (dockAnchor) frame.value = dockChatFrame(dockAnchor, viewport.value);
  saveFrame();
}, { flush: 'sync' });
function resizeHandleStyle(edge: ChatResizeEdge) {
  const style: Record<string, string> = { cursor: `${edge}-resize` };
  if (edge.length === 2) { style.width = '12px'; style.height = '12px'; }
  else if (edge === 'n' || edge === 's') { style.height = '6px'; style.left = '12px'; style.right = '12px'; }
  else { style.width = '6px'; style.top = '12px'; style.bottom = '12px'; }
  if (edge.includes('n')) style.top = '0'; if (edge.includes('s')) style.bottom = '0';
  if (edge.includes('w')) style.left = '0'; if (edge.includes('e')) style.right = '0';
  return style;
}
function beginFrameGesture(event: PointerEvent, mode: 'move' | 'resize', edge?: ChatResizeEdge) { if (event.button !== 0 || !event.isPrimary || (event.target as HTMLElement)?.closest('button, input, select, a')) return; event.preventDefault(); endFrameGesture(); const target = event.currentTarget as HTMLElement; gesture = { mode, edge, pointerId: event.pointerId, x: event.clientX, y: event.clientY, frame: { ...frame.value }, target }; try { target.setPointerCapture(event.pointerId); } catch { /* Window listeners also support platforms without pointer capture. */ } window.addEventListener('pointermove', moveFrameGesture); window.addEventListener('pointerup', endFrameGesture); window.addEventListener('pointercancel', endFrameGesture); window.addEventListener('blur', endFrameGesture); }
function moveFrameGesture(event: PointerEvent) { if (!gesture || event.pointerId !== gesture.pointerId) return; const dx = event.clientX - gesture.x, dy = event.clientY - gesture.y; frame.value = gesture.mode === 'move' ? fitChatFrame({ ...gesture.frame, x: gesture.frame.x + dx, y: gesture.frame.y + dy }, viewport.value) : resizeChatFrame(gesture.frame, gesture.edge || 'se', dx, dy, viewport.value); }
function endFrameGesture(event?: PointerEvent | Event) { if (!gesture || event && 'pointerId' in event && event.pointerId !== gesture.pointerId) return; const old = gesture; gesture = undefined; if (old.target.hasPointerCapture?.(old.pointerId)) old.target.releasePointerCapture(old.pointerId); window.removeEventListener('pointermove', moveFrameGesture); window.removeEventListener('pointerup', endFrameGesture); window.removeEventListener('pointercancel', endFrameGesture); window.removeEventListener('blur', endFrameGesture); if (autoDockWindow.value) updateDockAfterGesture(old.mode, old.frame); saveFrame(); }
function resizeKeydown(event: KeyboardEvent, edge: ChatResizeEdge) { if (event.key === 'Escape') { endFrameGesture(); return; } if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return; event.preventDefault(); const step = event.shiftKey ? 16 : 8, initial = frame.value; frame.value = resizeChatFrame(initial, edge, event.key === 'ArrowLeft' ? -step : event.key === 'ArrowRight' ? step : 0, event.key === 'ArrowUp' ? -step : event.key === 'ArrowDown' ? step : 0, viewport.value); if (autoDockWindow.value) updateDockAfterGesture('resize', initial); saveFrame(); }
watch(() => [mediaChat.open, mediaChat.revision], () => { if (mediaChat.open) void openEntry(); else { rememberDraft(); closePicker(); contextOpen.value = false; actionsOpen.value = false; actionSlash = null; sessionActionsOpen.value = false; historyMenuId.value = ''; cancelRename(); endFrameGesture(); previewOpen.value = false; toolReviewGeneration++; reviewingToolProposal.value = false; toolTaskReadGeneration++; toolTaskLoading = null; } }, { immediate: true });
watch(() => mediaChat.open, open => { if (open && !inputRegistered) { uiStore.pushInputHandler('MediaChat'); inputRegistered = true; } else if (!open && inputRegistered) { uiStore.removeInputHandler('MediaChat'); inputRegistered = false; } }, { immediate: true });
watch(() => libConfig._libraryId, libraryId => { if (activeLibrary.value && libraryId !== activeLibrary.value) { rememberDraft(); generation++; suggestionLibrary = ''; previewRows.value = []; previewError.value = ''; previewOpen.value = false; closeMediaChat(); } });
watch([() => activeLibrary.value, () => view.value?.session.id], () => { toolReviewGeneration++; reviewingToolProposal.value = false; toolTaskReadGeneration++; toolTaskLoading = null; toolTaskStates.value = {}; previewOpen.value = false; previewRows.value = []; });
watch(currentToolTasks, () => { void refreshToolTasks(); });
watch(() => [mention.value?.query, pickerQuery.value, candidateSource.value], () => { clearTimeout(candidateTimer); candidateTimer = setTimeout(() => { void loadCandidates(); }, 150); });
watch(sessionMedia, files => { if (mediaChat.open) void loadThumbnails(files); });
watch([selectedModelId, reasoningLevels, defaultReasoningEffort], normalizeReasoningChoice, { flush: 'sync' });
watch([draft, attachments, selectedModelId, reasoningEffort, toolsEnabled, toolScope, contextFields], () => { draftRevision++; rememberDraft(); }, { deep: true, flush: 'sync' });
watch(() => { const messages = renderedMessages.value, last = messages.at(-1); return [messages.length, last?.content, last?.reasoningContent, last?.phase, last?.status, last?.toolCalls]; }, () => { if (mediaChat.open && !showHistory.value) void scrollMessages(); });
onMounted(() => {
  viewport.value = { width: window.innerWidth, height: window.innerHeight };
  try {
    const stored = localStorage.getItem('lap.media-chat.window');
    const remembered = stored ? JSON.parse(stored) : {};
    frame.value = fitChatFrame(remembered, viewport.value);
    dockAnchor = autoDockWindow.value ? normalizeChatDockAnchor(remembered?.dock) || captureChatDockAnchor(remembered, viewport.value) : null;
  } catch { dockAnchor = autoDockWindow.value ? captureChatDockAnchor(frame.value, viewport.value) : null; }
  frameReady = true;
  fitFrame();
  window.addEventListener('resize', fitFrame);
  window.addEventListener('pointerdown', dismissActionsOutside);
  void listen<{ libraryId: string; jobId: string }>('media-ai-progress', event => {
    if (event.payload.libraryId === activeLibrary.value && currentToolTasks.value.has(event.payload.jobId)) void refreshToolTasks(event.payload.jobId);
  }).then(stop => { if (disposed) stop(); else offToolTasks = stop; }).catch(() => {});
  refreshTimer = setInterval(() => {
    if (mediaChat.open) void refreshToolTasks();
    if (mediaChat.open && activeRequestId.value && request.value?.id !== activeRequestId.value) void refreshCurrent().catch(reason => { error.value = String(reason); });
  }, 1500);
});
onUnmounted(() => {
  disposed = true; generation++; candidateGeneration++;
  offToolTasks?.();
  clearInterval(refreshTimer); clearTimeout(candidateTimer);
  endFrameGesture(); saveFrame();
  window.removeEventListener('resize', fitFrame);
  window.removeEventListener('pointerdown', dismissActionsOutside);
  if (inputRegistered) uiStore.removeInputHandler('MediaChat');
});
</script>

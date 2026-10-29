<template>
  <ScanHistoryDialog v-if="scanHistoryScope" :key="`${scanHistoryScope.libraryId}:${scanHistoryScope.albumId}:${scanHistoryScope.folderPath||''}`" :scope="scanHistoryScope" @cancel="scanHistoryScope=null" />

  <div
    ref="contentRootRef"
    tabindex="-1"
    class="relative flex-1 flex flex-col select-none outline-none"
    :class="{ 'opacity-50 pointer-events-none': uiStore.isInputActive('ManageLibraries') }"
    @focus="activateContentPane"
    @mousedown.capture="activateContentPane"
    @mouseenter="isContentHovered = true"
    @mouseleave="isContentHovered = false"
    @wheel.capture="handleContentWheel"
    @keydown="handleLocalKeyDown"
    @dragover.prevent
  >

    <!-- Loading overlay -->
    <transition name="fade">
      <div v-if="isProcessing" class="absolute inset-0 bg-base-100/50 flex items-center justify-center z-50 rounded-box">
        <span class="loading loading-dots text-primary"></span>
      </div>
    </transition>

    <!-- title bar -->
    <div
      v-if="!showWelcomeContent"
      class="absolute top-0 left-0 right-0 px-2 h-12 flex flex-row flex-nowrap items-center justify-between bg-base-300 z-30 overflow-hidden"
      data-tauri-drag-region
    >
      <!-- title -->
      <div class="mr-1 flex flex-row items-center gap-1 min-w-0 flex-1 overflow-hidden" data-tauri-drag-region>
        <TButton v-if="tempViewMode !== 'none'"
          :icon="IconPrev"
          :buttonSize="'medium'"
          :tooltip="$t('toolbar.tooltip.back')"
          :selected="true"
          @click="exitTempViewMode"
        />
        <component v-if="currentTitleIcon" 
          :is="currentTitleIcon" 
          class="t-icon-size-sm shrink-0"
          :class="{ 'cursor-pointer text-primary': tempViewMode !== 'none' }" 
          @click="handleTitleClick"
        />
        <Breadcrumb
          v-if="contentTitle"
          :items="titleBreadcrumbItems"
          :highlight="tempViewMode !== 'none'"
          size="large"
          class="min-w-0 overflow-hidden"
          @navigate="handleTitleBreadcrumbNavigate"
        />
        <span v-if="[SIDEBAR.OCR, SIDEBAR.OBJECTS].includes(config.main.sidebarIndex) && ocrPreviewDeferred" class="text-xs text-warning shrink-0" :title="t('ocr.preview_pending_hint')">{{ t('ocr.preview_pending') }}</span>
      </div>

      <!-- toolbar -->
      <div class="flex items-center gap-2 shrink-0">

        <TButton
          :icon="IconFileSearch"
          :tooltip="$t('structured_search.title')"
          :selected="isStructuredSearchView"
          :disabled="isScanStreamingMode"
          @click="openStructuredSearch(libConfig._libraryId)"
        />

        <!-- file type options -->
        <DropDownSelect
          v-if="!isStructuredSearchView && config.main.sidebarIndex!==SIDEBAR.OBJECTS && !(config.main.sidebarIndex===SIDEBAR.LIBRARY && libConfig.library.item===LIB_ITEM.MEDIA)"
          :icon="IconFilter"
          :options="fileTypeOptions"
          :multiSelect="true"
          :selectedValues="fileTypeSelectedValues"
          :summaryLabel="fileTypeSummaryLabel"
          :separatorsAfter="[0]"
          :disabled="isFixedAiResultView || tempViewMode === 'album' || isScanStreamingMode"
          :selected="activeFileTypeMask !== 0"
          @multi-select="handleFileTypeSelect"
        />

        <!-- sort type options -->
        <DropDownSelect
          :icon="isRandomSort ? IconSortingShuffle : (config.search.sortOrder === 0 ? IconSortingAsc : IconSortingDesc)"
          :options="toolbarSortOptions"
          :defaultIndex="toolbarSortType"
          :extendOptions="toolbarSortExtendOptions"
          :defaultExtendIndex="toolbarSortOrder"
          :extendDisabled="isRandomSort"
          :disabled="isSortControlDisabled"
          :selected="isSmartAlbumSortOverride"
          @select="handleSortTypeSelect"
        />

        <!-- global grouping options for query-backed grid views -->
        <DropDownSelect
          :icon="IconGroup"
          :options="toolbarGroupOptions"
          :defaultIndex="toolbarGroupIndex"
          :separatorsAfter="toolbarGroupOptions.length > 1 ? [0] : []"
          :disabled="isGroupControlDisabled"
          :selected="isSmartAlbumGroupOverride"
          @select="handleGroupSelect"
        />

        <!-- select and layout section -->
        <div class="flex flex-row items-center">
          <IconSeparator class="t-icon-size text-base-content/15" />
          
          <!-- refresh file list -->
          <!-- Bugfix: need to update album files when the album is updated -->
          <!-- <TButton
            :icon="IconRefresh"
            :tooltip="$t('toolbar.tooltip.refresh')"
            :disabled="isIndexing || showQuickView"
            @click="updateContent()"
          /> -->
          
          <!-- grid size slider -->
          <div class="flex flex-row items-center gap-2 px-2 shrink-0 group">
            <SliderInput v-model="gridSizePosition"
              :min="0" :max="100" :step="1" label="" :slider_width="80"
              :disabled="isMapView"
            />
          </div>

          <!-- grid styles cycle -->
          <TButton
            :icon="[IconCard, IconTile, IconJustified, IconJustified][config.settings.grid.style]"
            :iconStyle="{
              transform: `rotate(${config.settings.grid.style === 3 ? 90 : 0}deg)`,
            }"
            :tooltip="localeMsg.settings.grid.style_options[config.settings.grid.style]"
            :selected="isGridLayoutView"
            :disabled="tempViewMode === 'map'"
            @click="cycleGridStyle"
          />

          <!-- toggle filmstrip -->
          <TButton
            :icon="IconFilmstrip"
            :iconStyle="{ 
              transform: `rotate(${config.settings.grid.previewPosition === 0 ? 180 : (config.settings.grid.previewPosition === 2 ? 90 : (config.settings.grid.previewPosition === 3 ? 270 : 0))}deg)`, 
              transition: 'transform 0.3s ease-in-out' 
            }" 
            :tooltip="localeMsg.settings.grid.filmstrip_view.title"
            :selected="isFilmstripView"
            :disabled="tempViewMode === 'map'"
            @click="toggleFilmstripView"
          />

          <TButton
            :icon="IconMapDefault"
            :tooltip="$t('sidebar.map')"
            :selected="isMapView"
            @click="toggleMapView"
          />

          <IconSeparator class="t-icon-size text-base-content/15" />
          <!-- toggle select mode -->
          <TButton
            :icon="IconSelection"
            :tooltip="$t(selectionPanelOpen ? 'toolbar.filter.exit_select_mode' : 'toolbar.filter.select_mode')"
            :selected="selectionPanelOpen"
            :disabled="isScanStreamingMode || isMapVisible"
            @click="toggleSelectMode"
          />

          <!-- toggle dedup panel -->
          <TButton
            :icon="IconSimilar"
            :tooltip="$t('toolbar.tooltip.open_dedup')"
            :selected="isDedupPanelOpen"
            :disabled="isScanStreamingMode"
            @click="toggleDedupPanel"
          />

          <!-- toggle info panel -->
          <TButton
            :icon="IconInformation"
            :tooltip="isInfoPanelOpen ? $t('toolbar.tooltip.hide_info') : $t('toolbar.tooltip.show_info')"
            :shortcut="shortcut('meta.info')"
            :selected="isInfoPanelOpen"
            @click="toggleInfoPanel"
          />
        </div>
      </div>
    </div>

    <!-- progress bar -->
    <div v-if="showTopProgressBar" class="absolute top-11 left-0 right-0 z-50">
      <ProgressBar :key="scanProgressSession" :percent="topProgressPercent" />
    </div>

    <details v-if="activeSmartAlbum && (hasAiRules(activeSmartAlbum.query?.rules || []) || smartQueryError)" class="absolute top-12 right-2 z-40 max-w-[min(480px,90%)] max-h-[70vh] overflow-auto bg-base-200 rounded-box shadow" :open="!!smartQueryError || totalFileCount === 0">
      <summary class="cursor-pointer px-2 py-1 text-xs" :class="smartQueryError ? 'text-error' : ''">{{ smartQueryError ? $t('smart_albums.failed') : $t('smart_albums.ai_status') }}</summary>
      <SmartAlbumStatus :params="smartQueryParams(activeSmartAlbum)" :error="smartQueryError" @retry="refreshActiveSmartAlbum" />
    </details>
    <!-- content view -->
    <div ref="contentViewDiv" class="relative flex-1 flex flex-row overflow-hidden">
      <div class="relative flex-1 flex flex-row overflow-hidden">
        <PhotoMapView
          v-if="mapViewMounted"
          v-show="isMapContentVisible"
          class="mt-12 flex-1 min-w-0"
          :class="config.settings.showStatusBar ? 'mb-8' : 'mb-1'"
          :active="isMapContentVisible"
          :query-params="mapQueryParams"
          :query-source="currentQuerySource"
          :collection-id="currentCollectionId"
          :file-ids="currentSearchFileIds"
          :restore-view="mapTempViewState"
          :selected-file="fileList[selectedItemIndex]"
          @open-cluster="openMapClusterTempView"
          @select-file="selectMapFile"
          @preview-file="openMapPreviewFile"
          @restored="mapTempViewState = null"
        />
        <div v-if="!isMapContentVisible" ref="gridViewDiv"
          :class="[
            'flex-1 flex',
            gridViewLayoutClass,
            showFilmstripLayout ? (config.settings.showStatusBar ? 'mt-12 mb-8' : 'mt-12 mb-1') : ''
          ]"
        >
          <Transition name="selection-bar" appear>
          <div v-if="selectMode && !selectionPanelOpen" class="absolute top-16 right-4 z-20 flex items-center gap-1 rounded-box border border-base-content/10 bg-base-200/80 backdrop-blur-md px-2 py-1 shadow-sm"
            :class="{ 'pointer-events-none': uiStore.inputStack.length > 0 }"
            @pointerdown.stop @click.stop
          >
            <button class="flex items-center gap-2 rounded-box px-1 py-1 text-primary/70 hover:text-primary cursor-pointer"
              :aria-label="$t('toolbar.filter.show_selection_panel')"
              :title="$t('toolbar.filter.show_selection_panel')" @click="openSelectionPanel"
            >
              <span class="text-sm tabular-nums" aria-live="polite">
                {{ selectedCount > 0 ? $t('toolbar.filter.select_count', { count: selectedCount.toLocaleString() }) : $t('toolbar.filter.select_files') }}
              </span>
              <IconPanelMax class="size-4" />
            </button>
            <button class="btn btn-ghost btn-xs btn-square" :aria-label="$t('toolbar.filter.exit_select_mode')"
              :title="`${$t('toolbar.filter.exit_select_mode')} (Esc)`" @click="handleSelectMode(false)"
            ><IconClose class="size-4" /></button>
          </div>
          </Transition>
          <div class="relative" 
            :class="{ 'flex-1': !showFilmstripLayout }"
            :style="{ 
              height: (showFilmstripLayout && !isFilmstripVertical) ? itemSize + 'px' : '',
              width: (showFilmstripLayout && isFilmstripVertical) ? itemWidth + 'px' : ''
            }"
          >
            <!-- grid view -->
            <div ref="gridScrollContainerRef" class="absolute w-full h-full">
              <Welcome v-if="showWelcomeContent" />
              <GridView v-else ref="gridViewRef"
                :selected-item-index="selectedItemIndex"
                :fileList="gridRows"
                :timeline-data="timelineData"
                :sort-type="currentQueryParams.sortType"
                :selectMode="selectMode"
                :content-ready="contentReady"
                :empty-message="emptyFilesMessage"
                :empty-hint="emptyFilesHint"
                :layout-version="layoutVersion"
                :group-by="effectiveGroupBy"
                :group-selected-counts="groupSelectedCounts"
                :group-selection-loading="groupSelectionLoading"
                :folder-group-roots="folderGroupRoots"
                :query-source="currentQuerySource"
                :dedup-statuses="dedupStatuses"
                :dragged-file-ids="draggedFileIds"
                :grid-size="gridSize"
                :library-id="libConfig._libraryId"
                :marquee-enabled="isGridLayoutView && !isScanStreamingMode && !showQuickView"
                @item-clicked="handleItemClicked"
                @item-dblclicked="handleItemDblClicked"
                @item-select-toggled="handleItemSelectToggled"
                @item-action="handleItemAction"
                @selection-context-menu="handleSelectionContextMenu"
                @date-group-select="handleDateGroupSelect"
                @group-select-toggled="handleGroupSelectToggled"
                @visible-range-update="handleVisibleRangeUpdate"
                @scroll="handleGridScroll"
                @layout-update="handleLayoutUpdate"
                @grid-size-change="setGridSize"
                @item-drag-start="markContentInternalDrag"
                @item-drag="updateContentDragPosition"
                @item-drag-end="clearContentInternalDrag"
                @selection-box-start="beginBoxSelection"
                @selection-box-change="updateBoxSelection"
                @selection-box-end="finishBoxSelection"
              />
              <!-- Navigation buttons -->
              <div v-if="showFilmstripLayout"
                class="absolute z-10 inset-1 flex items-center justify-between pointer-events-none"
                :class="{ 'flex-col': isFilmstripVertical }"
              >
                <button 
                  :class="[
                    'preview-press-button p-2 rounded-full pointer-events-auto bg-base-100/30',
                    selectedItemIndex > 0 ? 'text-base-content/70 hover:text-base-content hover:bg-base-100/70 cursor-pointer' : 'text-base-content/30'
                  ]"
                  :disabled="selectedItemIndex <= 0"
                  @click="requestNavigate('prev')"
                  @dblclick.stop
                >
                  <span class="preview-press-icon flex pointer-events-none"><component :is="isFilmstripVertical ? IconArrowUp : IconLeft" class="w-8 h-8" /></span>
                </button>
                <button 
                  :class="[
                    'preview-press-button p-2 rounded-full pointer-events-auto bg-base-100/30',
                    selectedItemIndex < fileList.length - 1 ? 'text-base-content/70 hover:text-base-content hover:bg-base-100/70 cursor-pointer' : 'text-base-content/30'
                  ]"
                  :disabled="selectedItemIndex >= fileList.length - 1"
                  @click="requestNavigate('next')" 
                  @dblclick.stop
                >
                  <span class="preview-press-icon flex pointer-events-none"><component :is="isFilmstripVertical ? IconArrowDown : IconRight" class="w-8 h-8" /></span>
                </button> 
              </div>
            </div>
          </div>

          <div v-if="showFilmstripLayout"
            :class="isFilmstripVertical ? 'w-1 shrink-0' : 'h-1 shrink-0'"
          ></div>

          <!-- film strip preview -->
          <div v-if="showFilmstripLayout" ref="previewDiv"
            class="flex-1 bg-base-200 overflow-hidden"
          >
            <div v-if="selectedItemIndex >= 0 && selectedItemIndex < fileList.length"
              class="w-full h-full flex items-center justify-center"
            >
              <MediaViewer
                ref="filmStripMediaRef"
                :trackViews="!showQuickView && !isImageViewerWindowOpen && config.main.sidebarIndex !== SIDEBAR.HOME"
                :suppress-auto-navigator="infoNavigatorVisible"
              :object-class-key="objectBoxClassKey"
                @viewport-change="previewViewports.filmstrip = $event"
                @close="closeFilmstripFullScreen"
                :mode="1"
                :isFullScreen="false"
                :file="fileList[selectedItemIndex]"
                :nextFilePath="getNextImagePath(selectedItemIndex)"
                :hasPrevious="selectedItemIndex > 0"
                :hasNext="selectedItemIndex < fileList.length - 1"
                :fileIndex="selectedItemIndex"
                :fileCount="fileList.length"
                :isSlideShow="isSlideShow"
                :canSlideShow="true"
                :imageScale="imageScale"
                :imageMinScale="imageMinScale"
                :imageMaxScale="imageMaxScale"
                v-model:isZoomFit="filmStripZoomFit"
                @prev="performNavigate('prev')"
                @next="performNavigate('next')"
                @toggle-slide-show="toggleSlideShow"
                @scale="onScale"
                @item-action="handleItemAction"
                @view-background-change="setPreviewViewBackground"
                @slideshow-next="handleSlideshowNext"
              />
            </div>
          </div> <!-- film strip preview -->
        </div> <!-- grid view -->

        <!-- custom scrollbar -->
        <div v-if="!isMapVisible && !showWelcomeContent && !showFilmstripLayout && fileList.length > 0"
          class="mt-12 shrink-0" 
          :class="[ config.settings.showStatusBar ? 'mb-8' : 'mb-1' ]"
        >
          <ScrollBar
            :total="scrollbarTotal"
            :pageSize="scrollbarPageSize"
            :modelValue="scrollPosition"
            :markers="scrollbarMarkers"
            :selectedIndex="scrollbarSelectedIndex"
            @update:modelValue="handleScrollUpdate"
            @select-item="handleTimelineSelectItem"
          ></ScrollBar>
        </div>

        <!-- Quick View Overlay -->
        <div v-if="showQuickView && fileList[selectedItemIndex]"
          class="absolute inset-0 z-60 flex items-center justify-center bg-base-200/95 backdrop-blur-lg overflow-hidden"
          :class="[ config.settings.showStatusBar ? 'mt-12 mb-8': 'mt-12' ]"
        >
          <div
            class="relative w-full h-full flex items-center justify-center"
            @mousemove="handleQuickViewMouseMove"
            @mouseleave="handleQuickViewMouseLeave"
          >
            <MediaViewer
              ref="quickViewMediaRef"
              :trackViews="!isImageViewerWindowOpen && config.main.sidebarIndex !== SIDEBAR.HOME"
              :suppress-auto-navigator="infoNavigatorVisible"
                :object-class-key="objectBoxClassKey"
              @viewport-change="previewViewports.quick = $event"
              :mode="0"
              :isFullScreen="false"
              :file="fileList[selectedItemIndex]"
              :nextFilePath="getNextImagePath(selectedItemIndex)"
              :hasPrevious="selectedItemIndex > 0"
              :hasNext="selectedItemIndex < fileList.length - 1"
              :fileIndex="selectedItemIndex"
              :fileCount="fileList.length"
              :isSlideShow="isSlideShow"
              :canSlideShow="true"
              :imageScale="imageScale"
              :imageMinScale="imageMinScale"
              :imageMaxScale="imageMaxScale"
              :showOverlayNav="false"
              v-model:isZoomFit="quickViewZoomFit"
              @prev="performNavigate('prev')"
              @next="performNavigate('next')"
              @toggle-slide-show="toggleSlideShow"
              @scale="onScale"
              @item-action="handleItemAction"
              @view-background-change="setPreviewViewBackground"
              @close="closeQuickPreview()"
              @slideshow-next="handleSlideshowNext"
            />
            <div class="absolute inset-1 z-90 flex items-center justify-between pointer-events-none">
              <button
                :class="[
                  'preview-press-button p-2 rounded-full bg-base-100/30 transition-opacity duration-200',
                  quickViewHoverLeft
                    ? (selectedItemIndex > 0
                      ? 'opacity-100 pointer-events-auto text-base-content/70 hover:text-base-content hover:bg-base-100/70 cursor-pointer'
                      : 'opacity-30 pointer-events-none cursor-default text-base-content/30')
                    : 'opacity-0 pointer-events-none'
                ]"
                :disabled="selectedItemIndex <= 0"
                @pointerdown.stop
                @click.stop="requestNavigate('prev')"
                @dblclick.stop
              >
                <span class="preview-press-icon flex pointer-events-none"><IconLeft class="w-8 h-8" /></span>
              </button>
              <button
                :class="[
                  'preview-press-button p-2 rounded-full bg-base-100/30 transition-opacity duration-200',
                  quickViewHoverRight
                    ? (selectedItemIndex < fileList.length - 1
                      ? 'opacity-100 pointer-events-auto text-base-content/70 hover:text-base-content hover:bg-base-100/70 cursor-pointer'
                      : 'opacity-30 pointer-events-none cursor-default text-base-content/30')
                    : 'opacity-0 pointer-events-none'
                ]"
                :disabled="selectedItemIndex >= fileList.length - 1"
                @pointerdown.stop
                @click.stop="requestNavigate('next')"
                @dblclick.stop
              >
                <span class="preview-press-icon flex pointer-events-none"><IconRight class="w-8 h-8" /></span>
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- info panel splitter -->
      <div v-if="rightPanelLayoutVisible"
        class="w-1 shrink-0 mt-12 cursor-col-resize splitter-indicator splitter-vertical"
        :class="{
          'mb-8': config.settings.showStatusBar,
          'mb-1': !config.settings.showStatusBar,
          'splitter-dragging': isDraggingInfoPanel,
        }" 
        @mousedown="startDraggingInfoPanelSplitter"
      ></div>

      <!-- info panel -->
      <div
        v-if="rightPanelMounted"
        class="relative shrink-0"
        :style="{ width: rightPanelLayoutVisible ? activeRightPanelWidth + 'px' : '0px' }"
      >
        <div
          :class="[
            'absolute right-0 z-40 pr-1 transition-transform duration-200 ease-in-out',
            rightPanelVisualVisible ? 'translate-x-0' : 'translate-x-full pointer-events-none',
          ]"
          :style="{ width: activeRightPanelWidth + 'px', top: '3rem', bottom: config.settings.showStatusBar ? '2rem' : '0.25rem' }"
        >
          <DedupPane
            v-if="renderedRightPanelContent === 'dedup'"
            ref="dedupPaneRef"
            :selected-file-id="fileList[selectedItemIndex]?.id"
            :dedup-scan-key="dedupScanKey"
            :similar-scan-key="similarScanKey"
            :similarity-threshold="similarPhotoGroupingThreshold"
            :dedup-query-params="dedupQueryParams"
            :dedup-collection-id="dedupCollectionId"
            :dedup-file-ids="dedupFileIds"
            @close="config.rightPanel.show = false"
            @select-file="handleDedupSelectFile"
            @preview-file="handleDedupPreviewFile"
            @trash-selected-duplicates="handleDedupTrashSelectedDuplicates"
            @trash-all-duplicates="handleDedupTrashAllDuplicates"
            @trash-selected-similar="handleDedupTrashSelectedSimilar"
            @compare-selected-photos="handleDedupCompareSelectedPhotos"
            @culling-status-updated="handleDedupCullingStatusUpdated"
            @dedup-status-updated="dedupStatuses = $event"
          />
          <SelectionPanel
            v-else-if="renderedRightPanelContent === 'selection'"
            :file-count="fileList.length"
            :selected-files="selectionPreviewFiles"
            :selected-count="selectedCount"
            :originals-unavailable="selectionHasUnavailable"
            :selected-size="selectedSize"
            :query-source="currentQuerySource"
            :more-actions="selectionMenuItems"
            @close="handleSelectMode(false); config.rightPanel.show = false"
            @select-all="selectAllInCurrentList"
            @select-none="selectNoneInCurrentList"
            @select-invert="invertSelectionInCurrentList"
            @move-within-library="!currentOriginalsUnavailable() && (showMoveTo = true)"
            @move-to-folder="onMoveToFolder"
            @copy-to-folder="onCopyToFolder"
            @remove-from-collection="removeSelectedFromCollection"
            @trash="openTrashMsgbox()"
            @rename="clickRename"
            @favorite-all="selectModeSetFavorites(true)"
            @unfavorite-all="selectModeSetFavorites(false)"
            @set-rating-all="selectModeSetRatings"
            @set-culling-all="selectModeSetCullingFlags"
            @tag-all="clickTag"
            @add-to-collection="clickAddToCollection"
            @comment-all="openCommentEditor"
            @rotate-all="clickRotate"
            @unselect-file="unselectFileFromSelection"
            @more-action="action => action()"
            @more-action-menu="handleMoreActionMenu"
          />
          <FileInfo
            v-else-if="renderedRightPanelContent === 'info'"
            ref="fileInfoRef"
            :fileInfo="fileList[selectedItemIndex]"
            :editor-active="config.rightPanel.show && renderedRightPanelContent === 'info'"
            :refresh-file="refreshFileInfoFromPanel"
            :navigator-viewport="infoNavigatorViewport"
            @navigator-visible="infoNavigatorVisible = $event"
            @navigator-navigate="navigateInfoPreview"
            @navigator-zoom="getActivePreviewMediaRef()?.zoomNavigator?.($event)"
            @navigator-toggle-fit="getActivePreviewMediaRef()?.toggleZoomFit?.()"
            @close="checkUnsavedChanges(() => config.rightPanel.show = false)" 
            @success="onFileSaved(true, $event)"
            @failed="onFileSaved(false)"
            @toggleFavorite="toggleFavorite"
            @setRating="setSelectedFileRating"
            @setCulling="setSelectedFileCullingFlag"
            @rotate="handleInfoAction(clickRotate)"
            @quick-edit-tag="handleInfoAction(clickTag)"
            @quick-edit-collection="handleInfoAction(clickAddToCollection)"
            @quick-edit-comment="handleInfoAction(openCommentEditor)"
            @navigate-folder="handleInfoNavigateFolder"
            @navigate-metadata="handleNavigateMetadata"
            @navigate-person="handleNavigatePerson"
            @ocr-focus="!showFilmstripLayout && (showQuickView = true)"
            @object-focus="!showFilmstripLayout && (showQuickView = true)"
            @media-ai-seek="seekMediaAiVideo"
          />
        </div>
      </div>

    </div>

    <StatusBar
      v-if="config.settings.showStatusBar"
      :file-list="fileList"
      :selected-item-index="selectedItemIndex"
      :total-file-count="totalFileCount"
      :total-file-size="totalFileSize"
      :show-film-strip="showFilmstripLayout"
      :show-quick-view="showQuickView"
      :image-scale="imageDisplayScale"
      :scan-text="statusBarScanText"
      :show-update-icon="statusBarShowUpdateIcon"
      :is-update-animating="statusBarIsUpdateAnimating"
      :update-icon="statusBarUpdateIcon"
      :empty-message="statusBarEmptyMessage"
    />
  </div>

  <!-- rename -->
  <MessageBox
    v-if="showRenameMsgbox"
    :title="$t('msgbox.rename_file.title')"
    :showInput="true"
    :inputText="renamingFileName.name"
    :inputPlaceholder="$t('msgbox.rename_file.placeholder')"
    :inputExtension="renamingFileName.ext"
    :needValidateInput="true"
    :OkText="$t('msgbox.rename_file.ok')"
    :cancelText="$t('msgbox.cancel')"
    :errorMessage="errorMessage"
    @ok="onRenameFile"
    @cancel="showRenameMsgbox = false"
    @reset="errorMessage = ''"
  />

  <ThumbnailRegenerationDialog v-if="thumbnailRebuildRequest" :request="thumbnailRebuildRequest" @cancel="thumbnailRebuildRequest = null" />
  <RefreshFileInfoDialog
    v-if="fileRefreshSelection"
    :file-ids="fileRefreshSelection.ids"
    :library-id="fileRefreshSelection.libraryId"
    :finish-refresh="finishSelectedFileRefresh"
    @close="fileRefreshSelection = null"
  />

  <!-- move to -->
  <MoveTo
    v-if="showMoveTo"
    :title="`${$t('msgbox.move_to.title', { source: selectMode ? $t('toolbar.filter.select_count', { count: selectedCount.toLocaleString() }) : shortenFilename(fileList[selectedItemIndex]?.name || '', 32) })}`"
    :message="$t('msgbox.move_to.content')"
    :OkText="$t('msgbox.move_to.ok')" 
    :cancelText="$t('msgbox.cancel')"
    @ok="onMoveTo"
    @cancel="showMoveTo = false"
  />

  <FileConflictDialog
    v-if="fileConflictDialog.show"
    :name="fileConflictDialog.name"
    :destination="fileConflictDialog.destination"
    :showApplyAll="fileConflictDialog.showApplyAll"
    :allowReplace="fileConflictDialog.allowReplace"
    @resolve="resolveFileConflict"
  />

  <MessageBox v-if="showHideMsgbox" :isLoading="hideBusy" :title="$t('media_lifecycle.hide')" :message="$t('media_lifecycle.hide_confirm')" :okText="$t('media_lifecycle.hide')" :cancelText="$t('msgbox.cancel')" @ok="confirmHide" @cancel="showHideMsgbox=false;pendingHide=null" />
  <!-- move to trash -->
  <MessageBox
    v-if="showTrashMsgbox"
    :title="trashMsgboxTitle"
    :message="trashMsgboxMessage"
    :OkText="trashMsgboxOkText"
    :cancelText="$t('msgbox.cancel')"
    :warningOk="true"
    :checkboxText="$t('msgbox.permanent_delete.checkbox')"
    :checkboxChecked="deletePermanently"
    :isLoading="isTrashDeleting"
    @ok="handleTrashMsgboxOk"
    @cancel="closeTrashMsgbox"
    @checkbox-change="deletePermanently = $event"
  />

  <MessageBox
    v-if="showTrashFailedMsgbox"
    :title="$t('msgbox.trash_failed.title')"
    :message="trashFailedMsgboxMessage"
    :OkText="$t('msgbox.trash_failed.ok')"
    :cancelText="$t('msgbox.cancel')"
    :warningOk="true"
    @ok="confirmTrashFailedPermanentDelete"
    @cancel="cancelTrashFailedMsgbox"
  />

  <!-- opening many files in an external app -->
  <MessageBox
    v-if="showExternalOpenWarningMsgbox"
    :title="$t('msgbox.open_external_many.title', { count: pendingExternalOpen?.paths.length ?? 0 })"
    :message="$t('msgbox.open_external_many.message', { count: pendingExternalOpen?.paths.length ?? 0 })"
    :OkText="$t('msgbox.open_external_many.ok')"
    :cancelText="$t('msgbox.cancel')"
    :warningOk="true"
    @ok="confirmExternalOpen"
    @cancel="cancelExternalOpen"
  />

  <ExternalAppsDialog
    v-if="showExternalAppsDialog"
    @cancel="showExternalAppsDialog = false"
  />

  <!-- tag -->
  <TaggingDialog 
    v-if="showTaggingDialog"
    :fileIds="fileIdsToTag"
    position-key="edit-tag"
    @ok="updateFileHasTags"
    @states-changed="syncTagStates"
    @cancel="showTaggingDialog = false"
  />

  <!-- add to collection -->
  <AddToCollectionDialog
    v-if="showAddToCollectionDialog"
    :fileIds="fileIdsToAddToCollection"
    position-key="edit-collection"
    @applied="handleCollectionsAdded"
    @deleted="handleCollectionDeleted"
    @cancel="showAddToCollectionDialog = false"
  />

  <!-- comment -->
  <MessageBox
    v-if="showCommentMsgbox"
    :title="$t('msgbox.edit_comment.title')"
    :showInput="true"
    :inputText="commentInputText"
    :inputPlaceholder="$t('msgbox.edit_comment.placeholder')"
    :multiLine="true"
    :OkText="$t('msgbox.ok')"
    :cancelText="$t('msgbox.cancel')"
    position-key="edit-comment"
    @ok="onEditComment"
    @cancel="showCommentMsgbox = false"
  />

  <StructuredSearchDialogs />

  <!-- Unsaved Changes Confirmation -->
  <MessageBox
    v-if="showUnsavedChangesMsgbox"
    :title="$t('msgbox.unsaved_changes.title') || 'Unsaved Changes'"
    :message="$t('msgbox.unsaved_changes.message') || 'You have unsaved changes. Do you want to save them?'"
    :OkText="$t('msgbox.image_editor.save') || 'Save'"
    :cancelText="$t('msgbox.cancel')"
    :thirdText="$t('msgbox.discard') || 'Discard'"
    :warningThird="true"
    @ok="confirmSave"
    @cancel="cancelDiscard"
    @third="confirmDiscard"
  />

  <IndexRecoveryDialog
    v-if="showIndexRecoveryMsgbox"
    :title="indexRecoveryTitle"
    :message="indexRecoveryMessage"
    :fileLabel="indexRecoveryFileLabel"
    :filePath="recoverySkipFilePath"
    :continueText="indexRecoveryOkText"
    :skipLabel="indexRecoverySkipLabel"
    :cancelText="$t('msgbox.cancel')"
    @continue="confirmIndexRecoveryContinue"
    @cancel="cancelIndexRecovery"
  />

  <!-- Drag-drop warning -->
  <MessageBox
    v-if="showDropWarning"
    :title="$t('msgbox.drop_import.title')"
    :message="$t('msgbox.drop_import.message')"
    :cancelText="''"
    @ok="showDropWarning = false"
    @cancel="showDropWarning = false"
  />

  <ModalDialog v-if="fileImportVisible" :title="$t('import_files.title')" :width="480"
    @cancel="fileImportVisible = false">
    <div class="text-sm break-all mb-3">{{ fileImportTarget }}</div>
    <div v-if="isImportingFiles" role="status" aria-live="polite" class="text-sm">
      {{ $t('import_files.progress', { processed: fileImportProcessed, total: fileImportTotal }) }}
      <progress class="progress w-full mt-2" :value="fileImportProcessed" :max="fileImportTotal"></progress>
    </div>
    <div v-else class="text-sm mb-2">{{ $t('import_files.summary', { imported: fileImportSuccess, failed: fileImportErrors.length }) }}</div>
    <div v-if="fileImportErrors.length" class="max-h-64 overflow-y-auto text-xs text-error whitespace-pre-wrap break-all select-text">
      <div v-for="(error, index) in fileImportErrors" :key="index" class="mb-2">{{ error }}</div>
    </div>
  </ModalDialog>

  <!-- Drop overlay -->
  <div v-if="isDragOver && acceptDrops" class="drop-overlay">
    <div class="drop-overlay-content">
      <IconDownload class="w-16 h-16"/>
      <span>{{ $t('msgbox.drop_import.overlay') }}</span>
    </div>
  </div>

  <PrintImage ref="printImageView" />

  <!-- Pops a More-actions submenu parent's children (e.g. "Open in external
       app...") at the clicked toolbar button, reusing ContextMenu's submenu
       rendering and the same multi-select menu actions. The trigger is empty;
       it's opened at button coordinates by handleMoreActionMenu, and its popup
       teleports to <body>. -->
  <div class="hidden">
    <ContextMenu
      ref="moreActionMenuRef"
      :iconMenu="null"
      :menuItems="moreActionMenuItems"
    >
      <template #trigger><span></span></template>
    </ContextMenu>
  </div>
</template>

<script setup lang="ts">
import { applyMediaViewChange, mediaViewSortOptions } from '@/common/mediaViews';
import { useMediaViewEvents } from '@/common/mediaViewTracking';
import { mediaDisplayName, applyMediaTitleChange, mediaSortOptions } from '@/common/mediaTitle';
import { useMediaTitleEvents } from '@/common/mediaTitleEvents';
import { pendingChanges } from '@/common/pendingChanges';
import { startupWindowProxy } from '@/common/windowProxy';

import { isFaceRename, faceChangeAffectsPerson } from '@/common/faceUpdates';
import { defineAsyncComponent, ref, watch, computed, createVNode, onMounted, onBeforeUnmount, nextTick, render, markRaw } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { externalDragKind, updateExternalDrag, resetExternalDrag, finishExternalDrag } from '@/common/externalDrag';
import { getExternalDropUris, getExternalHttpDropUrls, getExternalFileDropPaths, fileUrlToPath, hasExternalDropIntent } from '@/common/externalDrop';

import PrintImage from '@/components/PrintImage.vue';
import { emit as tauriEmit, listen } from '@tauri-apps/api/event';
import { ask, open as openDialog } from '@tauri-apps/plugin-dialog';
import { WebviewWindow, getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { PREVIEW_WINDOW_FOCUS_RESTORED } from '@/common/previewWindow';
import { useI18n } from 'vue-i18n';
import { useToast } from '@/common/toast';
import { useUIStore } from '@/stores/uiStore';
import { getAlbum, getAllAlbums, recountAlbum, getQueryCountAndSum, getQueryTimeLine, getQueryFiles, getFilesByIds, getGroupedQueryRows, getGroupedFilePosition, getGroupFileIds, getQueryFileIds, syncAlbumFolderMtimes,
         getSmartQueryCountAndSum, getSmartQueryTimeLine, getSmartQueryFiles, getSmartGroupedQueryRows, getSmartGroupFileIds, getSmartQueryFileIds, getSmartQueryFilePosition,
         copyImages, renameFile, moveFile, moveFileOutsideLibrary, copyFile, withFileTransferBatch, deleteFile, deleteFilePermanently, batchDeleteFiles, editFileComment, getFileThumb, getFileThumbs, getFileInfo,
         setFileRotate, setFileFavorite, setFileRating, setFileCullingFlag, batchUpdateFileMetadata, getTagsForFile, getTagGroupName, searchSimilarImages, generateEmbedding,
         revealPath, getTagName, indexAlbum, listenIndexProgress, listenIndexFinished, setAlbumCover, setDesktopWallpaper,
         updateFileInfo, refreshFileInfo, getSupportedFormatExtensions, importFile, importUrl, importFileBytes, getDragPayload, importClipboard, addFileToDb, checkFileExists, checkFileAccessibility, selectFolder, getFacesForFile, listenFaceIndexProgress,
         openFilesWithApp, getAppConfig, getIndexRecoveryInfo, clearIndexRecoveryInfo, setLastSelectedItemIndex,
         dedupDelete, getQueryFilePosition, getFolderSearchExcluded,
         listCollections, createCollection, addFilesToCollection, removeFilesFromCollection, getFileCollections, getCollectionCountAndSum, getCollectionFiles, getCollectionGroupedQueryRows, getCollectionGroupFileIds, getCollectionQueryFileIds, fetchFolder, isDirectoryAccessible, checkAlbumAccessibility, addTagToFile, saveXmpMetadata } from '@/common/api';
import { config, libConfig } from '@/common/config';
import { createThumbnailScheduler } from '@/common/thumbnailScheduler';
import {
  gridSizeFromPosition,
  gridSizePositionFromGridSize,
  normalizeGridSizePosition,
  thumbnailProfile,
} from '@/common/thumbnailProfiles';
import { getShortcutLabel, matchesShortcut, ShortcutActionId, ShortcutPlatform, VIEW_BACKGROUND_SHORTCUTS } from '@/common/shortcuts';
import { getSmartTagById } from '@/common/smartTags';
import { clearFolderFileCounts, setFolderFileCount, getCountedFolderPaths } from '@/composables/useAlbumSelection';
import { createEmptyLibraryCounts } from '@/stores/libraryStore';
import { getAlbumScanState, getAlbumScanIcon, shouldAnimateAlbumScanIcon, scanWorkProgress } from '@/common/scanStatus';
import { scanJobs, albumScanRun, albumScanState, useScanTracking, pauseAlbumScan, applyScanEvent, refreshScanJobs } from '@/common/scanJobs';
import type { ScanScope } from '@/common/scanStatus';

import { startOcr } from '@/common/ocr';
import { normalizeOcrState } from '@/common/ocrUi';
import { startObjectIndex } from '@/common/objects';
import { normalizeObjectState, objectClassName, objectSelectionIds, resolveObjectTextClasses } from '@/common/objectUi';
import { CULLING, DATE_SORT, GROUP, LIB_ITEM, RATE, SIDEBAR, SELECTED_THUMBNAIL_LIMIT, RIGHT_PANEL_MIN_WIDTH, PANEL_ANIMATION_MS, MONTAGE_MAX_PHOTOS, LARGE_BATCH_CONFIRM_THRESHOLD, EXTERNAL_OPEN_WARNING_THRESHOLD } from '@/common/constants';
import { isWin, isMac, isLinux, setTheme, separator,
         formatFileSize, formatDate, getCalendarDateRange, formatFolderBreadcrumb, getThumbnailDataUrl, getAssetSrc, getPreviewUrl,
         getCachedThumbnailDataUrl,
         clearCachedThumbnailDataUrl,
         extractFileName, combineFileName, getFolderPath, getFolderName, getSelectOptions, 
         shortenFilename, getSlideShowInterval, getFullPath, normalizePathForCompare, isWithinRootPath, shouldUseBackendPreview } from '@/common/utils';

import DropDownSelect from '@/components/DropDownSelect.vue';
import ProgressBar from '@/components/ProgressBar.vue';
import GridView  from '@/components/GridView.vue';
const PhotoMapView = defineAsyncComponent(() => import('@/components/PhotoMapView.vue'));
import ContextMenu from '@/components/ContextMenu.vue';
import { MAX_NATIVE_DRAG_FILES, createDragPreview, isWindowDragEdge, isNativeFileDragActive, isReturningNativeFileDrag, startNativeFileDrag } from '@/common/nativeDrag';
import { isOriginalUnavailable, setFileAccessibility, setFolderAccessibility, requiresOriginalAction } from '@/common/availability';
import SmartAlbumStatus from './SmartAlbumStatus.vue';
import StructuredSearchDialogs from './StructuredSearchDialogs.vue';
import { homePhotoIds } from '@/common/homeDashboard';
import { structuredSearch, structuredSearchAlbum, openStructuredSearch } from '@/common/structuredSearch';
import { smartQueryParams, smartResultSignature, hasTimeSmartRules, hasSemanticRules, hasOcrRules, hasObjectRules, hasAiRules, isSmartFolder } from '@/common/smartAlbums';
import { setMediaHidden } from '@/common/api';
import { mediaTypeRules } from '@/common/mediaLifecycle';
import { useFileMenuItems } from '@/common/fileMenu';
import { setDlnaViewOrder } from '@/common/dlna';
import { faceSelectionIds } from '@/common/faceUi';
import { indexFaces } from '@/common/api';
import Welcome from '@/components/Welcome.vue';
import MediaViewer from '@/components/MediaViewer.vue';
import MessageBox from '@/components/MessageBox.vue';
import ModalDialog from '@/components/ModalDialog.vue';
import RefreshFileInfoDialog from '@/components/RefreshFileInfoDialog.vue';
import ThumbnailRegenerationDialog from '@/components/ThumbnailRegenerationDialog.vue';
import { thumbnailSelectionScope } from '@/common/thumbnailRebuild';
const thumbnailRebuildRequest = ref<any>(null);
import { fileInfoRevision, metadataRefreshRequiresQuery } from '@/common/fileInfoRefresh';
import { displayLocationFilterLabel } from '@/common/locationNames';
import IndexRecoveryDialog from '@/components/IndexRecoveryDialog.vue';
import MoveTo from '@/components/MoveTo.vue';
import TButton from '@/components/TButton.vue';
import TaggingDialog from '@/components/TaggingDialog.vue';
import AddToCollectionDialog from '@/components/AddToCollectionDialog.vue';
import ExternalAppsDialog from '@/components/ExternalAppsDialog.vue';
import FileInfo from '@/components/FileInfo.vue';
import Breadcrumb from '@/components/Breadcrumb.vue';
import DedupPane from '@/components/DedupPane.vue';
import SelectionPanel from '@/components/SelectionPanel.vue';
import { mediaAiSearch, rankedMediaHits, type MediaHit } from '@/common/mediaAi';
import { MEDIA_CHAT_DROP_EVENT } from '@/common/mediaChat';
import FileConflictDialog from '@/components/FileConflictDialog.vue';
import ScrollBar from '@/components/ScrollBar.vue';
import SliderInput from '@/components/SliderInput.vue';
import StatusBar from '@/components/StatusBar.vue';

import {
  IconFolders,
  IconGroup,
  IconFiles,
  IconFolder,
  IconFolderCog,
  IconTag,
  IconBolt,
  IconLocation,
  IconCameraAperture,
  IconLeft,
  IconRight,
  IconSeparator,
  IconCard,
  IconTile,
  IconJustified,
  IconFilmstrip,
  IconMapDefault,
  IconSelection,
  IconClose,
  IconInformation,
  IconSearch,
  IconBox,
  IconPhotoSearch,
  IconPersonSearch,
  IconFolderSearch,
  IconCalendar,
  IconCalendarMonth,
  IconCalendarDay,
  IconArrowUp,
  IconArrowDown,
  IconDownload,
  IconPrev,
  IconAdd,
  IconFlag,
  IconFlagFilled,
  IconFlagOff,
  IconStar,
  IconStarFilled,
  IconSimilar,
  IconCamera,
  IconHeartFilled,
  IconHistory,
  IconStack,
  IconFileSearch,
  IconPerson,
  IconSortingAsc,
  IconSortingDesc,
  IconSortingShuffle,
  IconFilter,
  IconPanelMax,
} from '@/common/icons';

const thumbnailPlaceholder = new URL('@/assets/images/image-file.png', import.meta.url).href;

const ScanHistoryDialog=defineAsyncComponent(()=>import('@/components/ScanHistoryDialog.vue'));

const props = defineProps({
  titlebar: String,
  libraryEmpty: Boolean
});

/// i18n
const { locale, messages, t } = useI18n();
const localeMsg = computed(() => messages.value[locale.value] as any);
const uiStore = useUIStore();

const contentTitle = ref("");
// Change only display labels when the UI language changes; keep query keys and photos intact.
watch([locale, () => libConfig.location.label], () => {
  if (config.main.sidebarIndex === SIDEBAR.LOCATION && libConfig.location.nodeId) {
    contentTitle.value = displayLocationFilterLabel(libConfig.location, locale.value);
  }
});

const titleSegments = computed(() => {
  if (!contentTitle.value) return [];
  const parts = contentTitle.value.split(' > ');
  return parts.length > 1 ? parts : [contentTitle.value];
});

const titleBreadcrumbItems = computed(() =>
  titleSegments.value.map((label, index) => ({
    label,
    path: String(index),
  }))
);

function handleTitleBreadcrumbNavigate(path: string) {
  const segmentIndex = Number(path);
  if (!Number.isInteger(segmentIndex)) return;

  if (segmentIndex === titleSegments.value.length - 1) {
    handleTitleClick();
    return;
  }

  handleBreadcrumbClick(segmentIndex);
}

function handleBreadcrumbClick(segmentIndex: number) {
  if (tempViewMode.value === 'album') {
    const segments = titleSegments.value;
    if (segmentIndex < 0 || segmentIndex >= segments.length - 1) return;

    const currentPath = currentQueryParams.value.searchFolder || '';
    if (!currentPath) return;

    const pathParts = currentPath.split(separator);
    const levelsToGoUp = segments.length - 1 - segmentIndex;
    const targetPath = pathParts.slice(0, Math.max(0, pathParts.length - levelsToGoUp)).join(separator);

    if (!targetPath || targetPath === currentPath) return;

    // Update breadcrumb title immediately to reflect parent navigation in temp album mode.
    contentTitle.value = segments.slice(0, segmentIndex + 1).join(' > ');

    const requestId = ++currentContentRequestId;
    clearContentRows();
    totalFileCount.value = 0;
    totalFileSize.value = 0;
    scrollPosition.value = 0;
    selectedItemIndex.value = 0;
    if (gridViewRef.value) {
      gridViewRef.value.scrollToPosition(0);
    }

    getFileList({ searchFolder: targetPath }, requestId);
    return;
  }

  const sidebarIndex = config.main.sidebarIndex;

  // Address breadcrumbs select the exact ancestor, never a guessed province name.
  if (sidebarIndex === SIDEBAR.LOCATION) {
    const path = libConfig.location.path || [];
    const parent = path[segmentIndex];
    if (parent?.nodeId) {
      const selectedPath = path.slice(0,segmentIndex + 1);
      Object.assign(libConfig.location,{nodeId:parent.nodeId,label:selectedPath.map(level => level.name).join(' > '),path:selectedPath});
    }
    return;
  }

  // Camera: clicking parent segment navigates to make only
  if (sidebarIndex === SIDEBAR.CAMERA) {
    if (segmentIndex === 0) {
      if (!config.camera.isCamera) {
        (libConfig.camera as any).lensModel = null;
      } else {
        libConfig.camera.model = null;
      }
    }
    return;
  }

  // Album folders: navigate to parent folder
  if (sidebarIndex === SIDEBAR.ALBUM) {
    const segments = titleSegments.value;
    // The first segment is the album/folder root name.
    // Remaining segments are relative path components.
    // To navigate to segmentIndex, we rebuild the folder path.
    const currentPath = libConfig.album.folderPath || '';

    if (!currentPath) return;

    // Split the current path to reconstruct the target path
    const pathParts = currentPath.split(separator);
    // segments[0] = root folder name, segments[1..] = path parts after root
    // segmentIndex=0 means navigate to root, so we go up (segments.length - 1 - segmentIndex) levels
    const levelsToGoUp = segments.length - 1 - segmentIndex;
    const targetPath = pathParts.slice(0, pathParts.length - levelsToGoUp).join(separator);

    // Update title immediately for responsive UI, defer folderPath update to
    // the expand-album-folder event handler so folderId stays consistent.
    contentTitle.value = segments.slice(0, segmentIndex + 1).join(' > ');
    tauriEmit('expand-album-folder', { albumId: libConfig.album.id, folderPath: targetPath });
  }
}

// album's folder mode
const isCurrentFolderExcluded = ref(false);
const isCurrentFolderMissing = ref(false);

const showFolderFiles = computed(() =>
  Boolean(config.main.sidebarIndex === SIDEBAR.ALBUM && libConfig.album.id && libConfig.album.id > 0 && !libConfig.album.selected)
);

const scanProgressSession = ref(0);

// div elements
const contentRootRef = ref<HTMLElement | null>(null);
const contentViewDiv = ref<HTMLDivElement | null>(null);
const gridViewDiv = ref<HTMLDivElement | null>(null);
const isContentHovered = ref(false);

// file list
const fileList = ref<any[]>([]);
const contentReady = ref(false);  // true after current view's content has loaded (empty or not), reset on navigation
const groupedRows = ref<any[]>([]);
const totalFileCount = ref(0);    // total files' count
const totalRowCount = ref(0);     // total render rows' count (group headers + files)
const totalFileSize = ref(0);     // total files' size

const selectedItemIndex = ref(-1);
const ocrPreviewDeferred = ref(false);
let pendingInitialSelectedIndex = -1;
let hasRestoredInitialSelection = false;

// mutil select mode
const selectMode = ref(false);
const selectionPanelOpen = ref(false);
watch(selectMode, (active) => { if (!active) selectionPanelOpen.value = false; }, { flush: 'sync' });
const selectedCount = ref(0);
const selectedSize = ref(0);  // selected files size
const selectionChunkSize = computed(() => Number(config.main?.selectionChunkSize) || 200);
const isRealFileItem = (item: any) => !!item && !item.isPlaceholder && typeof item.id === 'number';
const isPendingFileItem = (item: any) => !item || item.isPlaceholder;

function createVirtualFileSlots(count: number) {
  return new Array(Math.max(0, Number(count || 0))).fill(null);
}
const isGroupRow = (item: any) => item?.type === 'group';
const isItemRow = (item: any) => item?.type === 'item';
const selectedFileIds = markRaw(new Set<number>());
const selectionVersion = ref(0);
const selectedFilesVersion = ref(0);
const selectionPreviewFiles = ref<any[]>([]);

let selectionPreviewRequestId = 0;
type SelectionRestoreState = {
  selectedIds: Set<number>;
  selectedSizes: Map<number, number>;
  selectedSize: number;
  viewportFileId: number;
  fallbackFileId: number;
  refreshSizes?: boolean;
};
let pendingSelectionRestore: SelectionRestoreState | null = null;
let isRestoringSelection = false;
let pendingFocusedFileId: number | null = null;
let isRestoringFocusedFile = false;
const setItemSelected = (index: number, selected: boolean) => {
  if (index < 0 || index >= fileList.value.length) return;
  if (!fileList.value[index] && !ensureGroupedFileAtIndex(index)) return;
  const item = fileList.value[index];
  if (!item) return;
  const fileId = Number(item?.id || 0);
  const previous = fileId > 0 ? selectedFileIds.has(fileId) : Boolean(item?.isSelected);
  if (previous === selected) {
    item.isSelected = selected;
    return;
  }
  item.isSelected = selected;
  if (fileId > 0) {
    if (selected) selectedFileIds.add(fileId);
    else selectedFileIds.delete(fileId);
  }
  applySelectionDelta(item, selected ? 1 : -1);
};
const getActionableSelectedItems = () =>
  fileList.value.filter(item => isRealFileItem(item) && (item.isSelected || selectedFileIds.has(Number(item.id))));
const selectedFiles = computed(() => {
  selectedFilesVersion.value;
  return selectMode.value ? getActionableSelectedItems() : [];
});
const selectedMediaChatIds = computed(() => {
  selectedFilesVersion.value;
  const current = fileList.value[selectedItemIndex.value];
  const ids = selectMode.value ? Array.from(selectedFileIds) : isRealFileItem(current) ? [Number(current.id)] : [];
  return ids.filter(id => Number.isSafeInteger(id) && id > 0);
});

async function refreshSelectionPanelPreviews() {
  const requestId = ++selectionPreviewRequestId;
  if (!selectMode.value || selectedFileIds.size === 0) {
    selectionPreviewFiles.value = [];
    return;
  }

  const previewIds = Array.from(selectedFileIds).slice(0, SELECTED_THUMBNAIL_LIMIT);
  const filesById = new Map(getActionableSelectedItems().map(file => [Number(file.id), file]));
  const missingIds = previewIds.filter(id => !filesById.has(id));
  if (missingIds.length > 0) {
    const fetchedFiles = await getFilesByIds(missingIds);
    if (requestId !== selectionPreviewRequestId) return;
    for (const file of fetchedFiles || []) {
      const fileId = Number(file?.id || 0);
      if (fileId > 0) filesById.set(fileId, file);
    }
  }

  const previews = previewIds.map(id => filesById.get(id)).filter(Boolean);
  if (requestId !== selectionPreviewRequestId) return;
  selectionPreviewFiles.value = previews;
  await getFileListThumb(previews, { viewportBound: false });
}

watch([selectMode, selectedFilesVersion], () => {
  void refreshSelectionPanelPreviews();
});
const getMediaKind = (items: any[]): 'image' | 'video' | 'mixed' | 'empty' => {
  let hasImage = false;
  let hasVideo = false;
  for (const item of items) {
    if (item?.file_type === 1 || item?.file_type === 3) hasImage = true;
    else if (item?.file_type === 2) hasVideo = true;
    if (hasImage && hasVideo) return 'mixed';
  }
  if (hasImage) return 'image';
  if (hasVideo) return 'video';
  return 'empty';
};
const selectionMediaKind = computed(() => getMediaKind(selectedFiles.value));
type ImageViewerSession =
  | { mode: 'normal' }
  | { mode: 'compare'; files: any[] };

// A comparison window owns a snapshot of the selected files. Background content
// refreshes must not replace that source with the live file list.
const imageViewerSession = ref<ImageViewerSession>({ mode: 'normal' });
// True while the separate image viewer window exists, so selection changes can
// push updates without an async window lookup when no viewer is open.
const isImageViewerWindowOpen = ref(false);

function removeDeletedFilesFromImageViewerSession(fileIds: number[]) {
  const session = imageViewerSession.value;
  if (session.mode !== 'compare') return undefined;
  const deletedIds = new Set(fileIds);
  const files = session.files.filter(file => !deletedIds.has(Number(file.id)));
  if (files.length === session.files.length) return undefined;
  imageViewerSession.value = {
    ...session,
    files,
  };
  return files.length;
}

const fileRefreshSelection = ref<{ ids: number[]; libraryId: string; requestId: number; groupSizes: Map<string, number> } | null>(null);
const metadataRefreshDeferred = ref<{ libraryId: string; requestId: number } | null>(null);
function fileMutationReady() {
  if (!pendingChanges.hasPending()) return true;
  toast.warning(t('workflow_close.finish_edits_first'));
  return false;
}
function startSelectedFileRefresh() {
  if (fileRefreshSelection.value || selectedFileIds.size === 0) return;
  if (!fileMutationReady()) return;
  fileRefreshSelection.value = {
    ids: Array.from(selectedFileIds), libraryId: libConfig._libraryId, requestId: currentContentRequestId,
    groupSizes: new Map(Array.from(groupMetaMap, ([id, meta]) => [id, meta.size])),
  };
}
async function finishSelectedFileRefresh(results: Array<{ file: any; size_delta: number; skipped: boolean }>) {
  const selection = fileRefreshSelection.value;
  if (!selection || selection.libraryId !== libConfig._libraryId || results.length === 0) return;
  fileInfoRevision.value++;
  void tauriEmit('tags-changed');
  for (const { file, skipped } of results) {
    if (!skipped) clearCachedThumbnailDataUrl(file.id, config.settings.thumbnailSize);
  }
  if (selection.requestId !== currentContentRequestId) return;
  const scrollTop = gridViewRef.value?.getScrollTop();
  const loadedFiles = new Map(fileList.value.filter(isRealFileItem).map(file => [Number(file.id), file]));
  for (const { file, skipped } of results) {
    const loaded = loadedFiles.get(Number(file.id));
    if (!skipped && loaded) loaded.rawThumbnailStale = true;
  }
  if (results.some(({ file }) => metadataRefreshRequiresQuery(loadedFiles.get(Number(file.id)), file, getGroupingQueryParams()))) {
    // A changed date, location, media type or source stamp can change grouping
    // and filtered membership. Re-query while retaining the selection/viewport.
    await refreshContentAfterFileInfoChange();
    return;
  }
  // A cloud-only original can stay unread while its local XMP was imported.
  // Compare query metadata first, then patch without regenerating thumbnails.
  for (const { file: updated, skipped } of results) {
    const file = loadedFiles.get(Number(updated.id));
    if (skipped && file) Object.assign(file, updated);
  }
  results = results.filter(result => !result.skipped);
  if (!results.length) return;
  const refreshedFiles: any[] = [];
  const groupDeltas = new Map<string, number>();
  for (const { file: updated, size_delta } of results) {
    totalFileSize.value += size_delta;
    selectedSize.value += size_delta;
    const groupId = fileIdToGroupId.get(Number(updated.id));
    if (groupId) {
      groupDeltas.set(groupId, (groupDeltas.get(groupId) || 0) + size_delta);
    }
    const file = loadedFiles.get(Number(updated.id));
    if (!file) continue;
    Object.assign(file, updated, { rawThumbnailStale: true });
    refreshedFiles.push(file);
  }
  for (const [groupId, delta] of groupDeltas) {
    const meta = groupMetaMap.get(groupId);
    if (meta) meta.size = (selection.groupSizes.get(groupId) ?? meta.size) + delta;
  }
  for (const group of [...groupedTimelineGroups.value, ...groupedRows.value.filter(isGroupRow)]) {
    const id = String(group.groupId || group.group_id);
    if (groupDeltas.has(id)) group.size = groupMetaMap.get(id)?.size ?? group.size;
  }
  if (groupDeltas.size > 0) {
    const sizes = { ...groupSelectedSizeMap.value };
    for (const [id, delta] of groupDeltas) sizes[id] = Math.max(0, Number(sizes[id] || 0) + delta);
    groupSelectedSizeMap.value = sizes;
  }
  syncSelectionVersions();
  await nextTick();
  if (selection.libraryId !== libConfig._libraryId || selection.requestId !== currentContentRequestId) return;
  if (scrollTop !== undefined) gridViewRef.value?.scrollToPosition(scrollTop);
  await getFileListThumb(refreshedFiles, { bustCache: true, viewportBound: false });
}

const selectionAccessFiles = ref<any[]>([]);
const selectionAccessPending = ref(false);
const selectionHasUnavailable = computed(() => selectionAccessPending.value
  || selectionAccessFiles.value.some(isOriginalUnavailable)
  || getActionableSelectedItems().some(isOriginalUnavailable));
let selectionAccessRequest = 0;
let selectionAccessTimer: ReturnType<typeof setTimeout> | null = null;
watch([selectMode, selectedFilesVersion, () => libConfig._libraryId], () => {
  const request = ++selectionAccessRequest;
  if (selectionAccessTimer) clearTimeout(selectionAccessTimer);
  selectionAccessFiles.value = [];
  const ids = Array.from(selectedFileIds);
  selectionAccessPending.value = selectMode.value && ids.length > getActionableSelectedItems().length;
  if (!selectionAccessPending.value) return;
  const libraryId = libConfig._libraryId;
  selectionAccessTimer = setTimeout(async () => {
    const states: any[] = [];
    for (let i = 0; i < ids.length; i += 500) {
      const files = await getFilesByIds(ids.slice(i, i + 500));
      if (request !== selectionAccessRequest || libraryId !== libConfig._libraryId) return;
      if (!files || files.length !== Math.min(500, ids.length - i)) {
        selectionAccessPending.value = false;
        return;
      }
      for (const file of files) states.push({ album_id: file.album_id, album_accessible: file.album_accessible, file_path: file.file_path });
    }
    selectionAccessFiles.value = states;
    selectionAccessPending.value = false;
  }, 100);
});

const selectionMenuItems = useFileMenuItems(
  ref<any>(null),
  localeMsg,
  isMac,
  t,
  // Selection-menu actions act on the whole selection, never a single index.
  (action: string) => handleItemAction({ action, index: -1 }),
  {
    selectMode: ref(true),
    selectionMediaKind,
    selectionCount: selectedCount,
    selectionIds: () => Array.from(selectedFileIds),
    selectionHasUnavailable,
  },
);

const moreActionMenuRef = ref<InstanceType<typeof ContextMenu> | null>(null);
const moreActionMenuItems = ref<any[]>([]);

function handleSelectionContextMenu(index: number, event: MouseEvent) {
  if (!selectMode.value || uiStore.inputStack.length > 0) return;
  const openMenu = () => {
    if (!selectMode.value) return;
    const file = fileList.value[index];
    if (!isRealFileItem(file)) return;
    if (!selectedFileIds.has(Number(file.id))) {
      selectNoneInCurrentList();
      setItemSelected(index, true);
    }
    selectedItemIndex.value = index;
    moreActionMenuItems.value = selectionMenuItems.value;
    void nextTick(() => moreActionMenuRef.value?.open?.(event.clientX, event.clientY));
  };
  if (index === selectedItemIndex.value) openMenu();
  else checkUnsavedChanges(openMenu);
}
// Opens a More-actions submenu parent's children (e.g. "Open in external
// app...") at the clicked toolbar button, reusing ContextMenu's submenu
// rendering and the same multi-select menu actions.
function handleMoreActionMenu(item: any, event: MouseEvent) {
  const children = item?.children;
  if (!Array.isArray(children) || children.length === 0) return;
  moreActionMenuItems.value = children;
  const rect = (event.currentTarget as HTMLElement | null)?.getBoundingClientRect();
  const x = rect ? rect.left : event.clientX;
  const y = rect ? rect.bottom : event.clientY;
  // Let the bound menuItems prop flush to the child before it measures/opens.
  void nextTick(() => moreActionMenuRef.value?.open?.(x, y));
}

const groupedModeActive = ref(false);
const selectedFolderHasChildren = ref(true);
const gridRows = computed(() => groupedModeActive.value && !isFilmstripView.value ? groupedRows.value : fileList.value);
const groupFileIdsCache = new Map<string, number[]>();
const groupedTimelineGroups = ref<any[]>([]);
const folderGroupRoots = ref<Array<{ path: string; name?: string }>>([]);
const groupSelectedCountMap = ref<Record<string, number>>({});
const groupSelectedSizeMap = ref<Record<string, number>>({});
const groupSelectedCounts = computed(() => groupSelectedCountMap.value);
const groupSelectionLoading = ref<Record<string, boolean>>({});
const fileIdToGroupId = new Map<number, string>();
const groupMetaMap = new Map<string, { count: number; size: number }>();

const FILE_OPERATION_CONCURRENCY = 8;

function clearSelectionForFileListUpdate() {
  if (pendingSelectionRestore) {
    clearLoadedSelectionFlags();
    lastSelectedIndex.value = -1;
    keyboardSelectionAnchorIndex.value = -1;
    groupSelectedCountMap.value = {};
    groupSelectedSizeMap.value = {};
    return;
  }
  selectMode.value = false;
  resetSelectionSummary();
  lastSelectedIndex.value = -1;
  keyboardSelectionAnchorIndex.value = -1;
  groupSelectedCountMap.value = {};
  groupSelectedSizeMap.value = {};
}

function captureSelectionForFileListRefresh() {
  if (!selectMode.value) return;

  const selectedSizes = new Map<number, number>();
  for (const file of fileList.value) {
    const fileId = Number(file?.id || 0);
    if (fileId > 0 && selectedFileIds.has(fileId)) {
      selectedSizes.set(fileId, Number(file?.size || 0));
    }
  }
  const activeFileId = Number(fileList.value[selectedItemIndex.value]?.id || 0);
  pendingSelectionRestore = {
    selectedIds: new Set(selectedFileIds),
    refreshSizes: Boolean(fileRefreshSelection.value),
    selectedSizes,
    selectedSize: selectedSize.value,
    viewportFileId: activeFileId,
    fallbackFileId: Number(selectedFileIds.values().next().value || 0),
  };
}

async function restoreSelectionAfterFileListRefresh() {
  const restoreState = pendingSelectionRestore;
  if (!restoreState || isRestoringSelection || !contentReady.value) return;
  if (fileList.value.length === 0) {
    pendingSelectionRestore = null;
    resetSelectionSummary();
    // Only metadata refresh promises to keep multi-select on an empty result.
    // Other refresh flows must not override the current mode here.
    if (restoreState.refreshSizes) selectMode.value = true;
    return;
  }

  isRestoringSelection = true;
  const requestId = currentContentRequestId;
  let retryForNewerRefresh = false;
  try {
    const currentIds = await getCurrentQueryFileIds();
    if (requestId !== currentContentRequestId || pendingSelectionRestore !== restoreState) {
      retryForNewerRefresh = true;
      return;
    }

    // A failed ID query must not look like an empty query: doing so clears a
    // valid selection during a transient backend error.
    if (!Array.isArray(currentIds)) return;
    const currentFileIds = currentIds;
    const availableIds = new Set(
      currentFileIds
        .map(Number)
        .filter(id => Number.isFinite(id) && id > 0),
    );
    const nextSelectedIds = new Set(
      Array.from(restoreState.selectedIds).filter(id => availableIds.has(id)),
    );

    // A metadata refresh may change sizes, including files outside loaded rows.
    if (restoreState.refreshSizes) {
      const ids = Array.from(nextSelectedIds);
      for (let offset = 0; offset < ids.length; offset += 200) {
        const files = await getFilesByIds(ids.slice(offset, offset + 200));
        if (requestId !== currentContentRequestId || pendingSelectionRestore !== restoreState) {
          retryForNewerRefresh = true;
          return;
        }
        if (!Array.isArray(files)) return;
        for (const file of files) restoreState.selectedSizes.set(Number(file.id), Number(file.size || 0));
      }
    }

    const viewportFileId = availableIds.has(restoreState.viewportFileId)
      ? restoreState.viewportFileId
      : (nextSelectedIds.has(restoreState.fallbackFileId)
        ? restoreState.fallbackFileId
        : Number(nextSelectedIds.values().next().value || 0));
    const fallbackIndex = viewportFileId <= 0
      ? -1
      : (groupedModeActive.value
        ? await resolveGroupedFileIndex(viewportFileId)
        : currentFileIds.findIndex(
            (fileId: number) => Number(fileId) === viewportFileId,
          ));
    if (requestId !== currentContentRequestId || pendingSelectionRestore !== restoreState) {
      retryForNewerRefresh = true;
      return;
    }
    if (fallbackIndex === null) return;
    const targetIndex = fallbackIndex >= 0 ? fallbackIndex : 0;
    if (targetIndex >= 0) {
      selectedItemIndex.value = targetIndex;
      await nextTick();
      if (groupedModeActive.value) {
        await scrollToGroupedFile(targetIndex);
      } else {
        gridViewRef.value?.scrollToItem(targetIndex);
      }
    }
    pendingFocusedFileId = null;

    selectedFileIds.clear();
    for (const fileId of nextSelectedIds) selectedFileIds.add(fileId);
    for (const file of fileList.value) {
      if (isRealFileItem(file)) file.isSelected = selectedFileIds.has(Number(file.id));
    }
    selectedCount.value = selectedFileIds.size;
    selectedSize.value = !restoreState.refreshSizes && nextSelectedIds.size === restoreState.selectedIds.size
      ? restoreState.selectedSize
      : Array.from(selectedFileIds).reduce(
          (total, fileId) => total + Number(restoreState.selectedSizes.get(fileId) || 0),
          0,
        );
    selectMode.value = true;
    syncSelectionVersions();
    pendingSelectionRestore = null;
  } catch (error) {
    console.error('restoreSelectionAfterFileListRefresh error:', error);
  } finally {
    isRestoringSelection = false;
    if (retryForNewerRefresh && pendingSelectionRestore && fileList.value.length > 0) {
      void restoreSelectionAfterFileListRefresh();
    }
  }
}

function rememberFocusedFileForPresentationRefresh() {
  const fileId = Number(fileList.value[selectedItemIndex.value]?.id || 0);
  pendingFocusedFileId = fileId > 0 ? fileId : null;
}

async function restoreFocusedFileAfterListRefresh() {
  const fileId = pendingFocusedFileId;
  if (!fileId || isRestoringFocusedFile || fileList.value.length === 0) return;

  isRestoringFocusedFile = true;
  const requestId = currentContentRequestId;
  let retryForNewerRefresh = false;
  try {
    const fileIndex = groupedModeActive.value
      ? await resolveGroupedFileIndex(fileId)
      : await getCurrentQueryFilePosition(fileId);
    if (requestId !== currentContentRequestId || pendingFocusedFileId !== fileId) {
      retryForNewerRefresh = true;
      return;
    }
    // `undefined` means the position request failed. Preserve the existing
    // focus and wait for the next list refresh instead of resetting it.
    if (fileIndex === undefined || fileIndex === null) return;
    pendingFocusedFileId = null;
    if (fileIndex < 0) {
      // The refreshed filter no longer contains the focused file. Leave the
      // normal first-item fallback in place rather than retaining a stale index.
      selectedItemIndex.value = totalFileCount.value > 0 ? 0 : -1;
      return;
    }
    selectedItemIndex.value = fileIndex;
    await nextTick();
    if (groupedModeActive.value) {
      await scrollToGroupedFile(fileIndex);
    } else {
      gridViewRef.value?.scrollToItem(fileIndex);
    }
  } catch (error) {
    console.error('restoreFocusedFileAfterListRefresh error:', error);
  } finally {
    isRestoringFocusedFile = false;
    if (retryForNewerRefresh && pendingFocusedFileId && fileList.value.length > 0) {
      void restoreFocusedFileAfterListRefresh();
    }
  }
}

async function waitForGroupedGridLayout() {
  await nextTick();
  await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
  gridViewRef.value?.refreshLayout();
  await nextTick();
}

async function scrollToGroupedFile(fileIndex: number) {
  const rowIndex = getGroupedRowIndexForFileIndex(fileIndex);
  if (rowIndex < 0) return;

  await waitForGroupedGridLayout();
  gridViewRef.value?.scrollToRowIndex(rowIndex);

  if (isGeometryGridStyle.value) {
    await fetchGroupedRowsRange(
      Math.max(0, rowIndex - selectionChunkSize.value),
      rowIndex + selectionChunkSize.value,
    );
    await waitForGroupedGridLayout();
    gridViewRef.value?.scrollToRowIndex(rowIndex);
  }
}

watch([fileList, contentReady], () => {
  if (!contentReady.value) return;
  if (pendingSelectionRestore) {
    void restoreSelectionAfterFileListRefresh();
  } else if (pendingFocusedFileId && fileList.value.length > 0) {
    void restoreFocusedFileAfterListRefresh();
  }
});

function clearLoadedSelectionFlags() {
  for (const file of fileList.value) {
    if (file) file.isSelected = false;
  }
}

function resetGroupingState() {
  groupedModeActive.value = false;
  groupedRows.value = [];
  totalRowCount.value = 0;
  groupFileIdsCache.clear();
  groupedTimelineGroups.value = [];
  groupSelectedCountMap.value = {};
  groupSelectedSizeMap.value = {};
  groupSelectionLoading.value = {};
  fileIdToGroupId.clear();
  groupMetaMap.clear();
}

function clearContentRows() {
  resetGroupingState();
  fileList.value = [];
}

function createViewBackup() {
  return {
    fileList: [...fileList.value],
    groupedModeActive: groupedModeActive.value,
    groupedRows: [...groupedRows.value],
    totalRowCount: totalRowCount.value,
    groupedTimelineGroups: [...groupedTimelineGroups.value],
    folderGroupRoots: [...folderGroupRoots.value],
    groupMetaEntries: Array.from(groupMetaMap.entries()),
    fileIdToGroupIdEntries: Array.from(fileIdToGroupId.entries()),
    totalFileCount: totalFileCount.value,
    totalFileSize: totalFileSize.value,
    contentTitle: contentTitle.value,
    selectedItemIndex: selectedItemIndex.value,
    scrollPosition: scrollPosition.value,
    timelineData: [...timelineData.value],
    currentQueryParams: { ...currentQueryParams.value },
    currentQuerySource: currentQuerySource.value,
    currentSmartQueryParams: currentSmartQueryParams.value ? { ...currentSmartQueryParams.value } : null,
    currentCollectionId: currentCollectionId.value,
    currentSearchFileIds: [...currentSearchFileIds.value],
    currentMapSearchExactMembers,
    scrollTop: gridViewRef.value ? gridViewRef.value.getScrollTop() : 0,
  };
}

function getActiveCustomSmartAlbum() {
  if (libConfig.smartAlbum.type !== 'custom' || !libConfig.smartAlbum.id) return null;
  return (libConfig.smartAlbums || []).find(
    (item: any) => item.id === libConfig.smartAlbum.id && !isSmartFolder(item)
  ) || null;
}

const isCollectionPane = computed(() => libConfig.activePane === 'collection');

const RANDOM_SORT = 7;
const RANDOM_SEED_MODULUS = 2_147_483_647;

function createRandomSeed() {
  const values = new Uint32Array(1);
  crypto.getRandomValues(values);
  return values[0] % RANDOM_SEED_MODULUS;
}

const isStructuredSearchView = computed(() =>
  !isCollectionPane.value && config.main.sidebarIndex === SIDEBAR.SEARCH
  && structuredSearch.active && structuredSearch.libraryId === libConfig._libraryId,
);
const activeStructuredAlbum = computed(() => isStructuredSearchView.value
  ? structuredSearchAlbum(libConfig._libraryId, t('structured_search.title'), config.search.sortType, config.search.sortOrder, config.search.groupBy)
  : null);
const activeSmartAlbum = computed(() => activeStructuredAlbum.value || (
  !isCollectionPane.value && config.main.sidebarIndex === SIDEBAR.SMART_ALBUM
    ? getActiveCustomSmartAlbum()
    : null),
);

const isRandomSort = computed(() => {
  const sortType = activeSmartAlbum.value
    ? activeSmartAlbum.value.sort?.type ?? config.search.sortType
    : config.search.sortType;
  return Number(sortType) === RANDOM_SORT;
});

const effectiveGroupBy = computed(() => {
  if (isRandomSort.value) return GROUP.NONE;
  const smartAlbum = getActiveCustomSmartAlbum();
  return Number(
    !isCollectionPane.value && config.main.sidebarIndex === SIDEBAR.SMART_ALBUM && smartAlbum
      ? smartAlbum.group?.type ?? config.search.groupBy ?? GROUP.NONE
      : config.search.groupBy ?? GROUP.NONE
  );
});

const isSearchGroupingView = computed(() =>
  config.main.sidebarIndex === SIDEBAR.SEARCH && !isStructuredSearchView.value
);

const isSubjectGroupingView = computed(() =>
  config.main.sidebarIndex === SIDEBAR.LIBRARY &&
  libConfig.library.item === LIB_ITEM.SUBJECTS
);

const isGroupingControlAvailable = computed(() =>
  !isFilmstripView.value &&
  !isScanStreamingMode.value &&
  tempViewMode.value === 'none' &&
  (libConfig.activePane === 'collection' || (
    !isSearchGroupingView.value &&
    !isSubjectGroupingView.value
  ))
);

function isGroupingSupportedForCurrentView() {
  return (
    effectiveGroupBy.value > 0 &&
    !isFilmstripView.value &&
    !isScanStreamingMode.value &&
    tempViewMode.value === 'none' &&
    currentQuerySource.value !== 'search'
  );
}

function formatFolderGroupLabel(label: string) {
  const folderPath = String(label || '').trim();
  if (!folderPath || folderPath === 'Unknown folder') return folderPath;

  const root = [...folderGroupRoots.value]
    .filter(root => root.path && isWithinRootPath(folderPath, root.path))
    .sort((a, b) => b.path.length - a.path.length)[0];
  if (!root) return folderPath;

  return formatFolderBreadcrumb(folderPath, root.path, root.name || '');
}

function formatDateGroupLabel(groupBy: number, label: string) {
  const rawLabel = String(label || '').trim();

  const timestamp = Number(rawLabel);
  // Local day buckets around the Unix epoch can be negative (for example,
  // 1970-01-01 in a positive-offset timezone), but are still valid dates.
  if (!Number.isFinite(timestamp)) return rawLabel;

  const value = new Date(timestamp * 1000);
  const year = value.getFullYear();
  const month = value.getMonth() + 1;
  const date = value.getDate();
  if (!year || !month || !date) return rawLabel;

  switch (groupBy) {
    case GROUP.DAY:
      return formatDate(year, month, date, 'date_long');
    case GROUP.MONTH:
      return formatDate(year, month, 1, 'month');
    case GROUP.YEAR: {
      if (config.main.sidebarIndex === SIDEBAR.LIBRARY && libConfig.library.item === LIB_ITEM.TODAY) {
        const today = new Date();
        return formatDate(year, today.getMonth() + 1, today.getDate(), 'date_long');
      }
      return formatDate(year, 1, 1, 'year');
    }
    default:
      return rawLabel;
  }
}

function ratingLabelKey(rating: number) {
  const keyMap: Record<number, string> = {
    5: 'five_stars',
    4: 'four_stars',
    3: 'three_stars',
    2: 'two_stars',
    1: 'one_star',
  };
  return keyMap[rating] || '';
}

function formatRatingGroupLabel(label: string) {
  const rating = Number(label || 0);
  if (rating === RATE.UNRATED) return localeMsg.value.rating.unrated;
  const key = ratingLabelKey(rating);
  return key ? localeMsg.value.rating[key] : label || '';
}

function formatCullingGroupLabel(label: string) {
  switch (Number(label || 0)) {
    case 1:
      return localeMsg.value.culling.picks;
    case 2:
      return localeMsg.value.culling.rejected;
    default:
      return localeMsg.value.culling.unreviewed;
  }
}

function formatGroupLabel(label: string) {
  const unknownGroupLabels: Record<string, string> = {
    'unknown-location': localeMsg.value.search?.unknown_location || 'Unknown location',
    'unknown-camera': localeMsg.value.search?.unknown_camera || 'Unknown camera',
    'unknown-lens': localeMsg.value.search?.unknown_lens || 'Unknown lens',
  };
  if (unknownGroupLabels[label]) return unknownGroupLabels[label];

  const groupBy = Number(effectiveGroupBy.value || 0);
  if (groupBy === GROUP.LOCATION) {
    try {const value = JSON.parse(label); if (Array.isArray(value)) return value[0] || unknownGroupLabels['unknown-location'];} catch { /* unknown/unresolved label */ }
  }
  if (groupBy === GROUP.DAY || groupBy === GROUP.MONTH || groupBy === GROUP.YEAR) return formatDateGroupLabel(groupBy, label);
  if (groupBy === GROUP.FOLDER) return formatFolderGroupLabel(label);
  if (groupBy === GROUP.RATING) return formatRatingGroupLabel(label);
  if (groupBy === GROUP.CULLING) return formatCullingGroupLabel(label);
  if (groupBy === GROUP.FILE_TYPE) {
    const labels = localeMsg.value.toolbar.filter?.file_type_options || [];
    const fileTypeLabels: Record<string, string> = {
      image: labels[1] || 'Photos',
      raw: labels[2] || 'RAW Files',
      video: labels[3] || 'Videos',
      other: localeMsg.value.toolbar.filter?.file_type_other || 'Other',
    };
    return fileTypeLabels[label] || label || '';
  }
  return label || '';
}

// Resolve which per-view category sort applies to a given content grouping.
// Only location/camera/lens grouping uses category_sort on the backend; the
// lens list shares the Camera view's sort. Tag/person sorts drive their own
// sidebars and never reach the content query.
function categorySortForGroup(groupBy: number) {
  switch (Number(groupBy)) {
    case GROUP.LOCATION: return Number(config.settings.locationSort || 0);
    case GROUP.CAMERA:
    case GROUP.LENS: return Number(config.settings.cameraSort || 0);
    default: return 0;
  }
}

function getGroupingQueryParams() {
  const baseParams = currentQuerySource.value === 'smart' && currentSmartQueryParams.value
    ? currentSmartQueryParams.value
    : currentQueryParams.value;
  const calendarSort = config.main.sidebarIndex === SIDEBAR.LIBRARY && libConfig.library.item === LIB_ITEM.TODAY
    ? DATE_SORT.TAKEN_DESC
    : Number(config.settings.calendarSort || 0);
  return {
    ...baseParams,
    groupBy: effectiveGroupBy.value,
    folderSort: Number(config.settings.folderSort || 0),
    calendarSort,
    categorySort: categorySortForGroup(effectiveGroupBy.value),
  };
}

async function ensureFolderGroupRoots() {
  if (folderGroupRoots.value.length > 0) return;
  try {
    const albums = await getAllAlbums();
    folderGroupRoots.value = Array.isArray(albums)
      ? albums
          .map((album: any) => ({
            path: String(album?.path || ''),
            name: String(album?.name || ''),
          }))
          .filter(album => album.path)
      : [];
  } catch (error) {
    console.error('ensureFolderGroupRoots error:', error);
    folderGroupRoots.value = [];
  }
}

function updateGroupSelectedCount(groupId: string, count: number) {
  groupSelectedCountMap.value = {
    ...groupSelectedCountMap.value,
    [groupId]: Math.max(0, count),
  };
}

function updateGroupSelectedSize(groupId: string, size: number) {
  groupSelectedSizeMap.value = {
    ...groupSelectedSizeMap.value,
    [groupId]: Math.max(0, size),
  };
}

function syncSelectionVersions() {
  selectionVersion.value++;
  selectedFilesVersion.value++;
}

function applySelectionDelta(file: any, delta: number) {
  if (!isRealFileItem(file) || delta === 0) return;
  selectedCount.value = Math.max(0, selectedCount.value + delta);
  selectedSize.value = Math.max(0, selectedSize.value + delta * Number(file.size || 0));
  updateGroupSelectedCountForFile(file, delta);
  syncSelectionVersions();
}

function updateGroupSelectedCountForFile(file: any, delta: number) {
  const fileId = Number(file?.id || 0);
  if (!fileId) return;
  const groupId = fileIdToGroupId.get(fileId);
  if (!groupId) return;
  const meta = groupMetaMap.get(groupId);
  const current = Number(groupSelectedCountMap.value[groupId] || 0);
  const maxCount = Number(meta?.count || Number.MAX_SAFE_INTEGER);
  updateGroupSelectedCount(groupId, Math.min(maxCount, Math.max(0, current + delta)));

  const currentSize = Number(groupSelectedSizeMap.value[groupId] || 0);
  const fileSize = Number(file?.size || 0);
  const maxSize = Number(meta?.size || Number.MAX_SAFE_INTEGER);
  updateGroupSelectedSize(groupId, Math.min(maxSize, Math.max(0, currentSize + delta * fileSize)));
}

function resetSelectionSummary() {
  selectedCount.value = 0;
  selectedSize.value = 0;
  selectedFileIds.clear();
  syncSelectionVersions();
}

function recomputeLoadedGroupSelectedCounts() {
  const next: Record<string, number> = {};
  const nextSize: Record<string, number> = {};
  for (const groupId of groupMetaMap.keys()) {
    next[groupId] = 0;
    nextSize[groupId] = 0;
  }

  for (const file of fileList.value) {
    if (!isRealFileItem(file)) continue;
    const fileId = Number(file.id || 0);
    const groupId = fileIdToGroupId.get(fileId);
    if (!groupId) continue;
    if (file.isSelected || selectedFileIds.has(fileId)) {
      const maxCount = Number(groupMetaMap.get(groupId)?.count || Number.MAX_SAFE_INTEGER);
      next[groupId] = Math.min(maxCount, Number(next[groupId] || 0) + 1);
      const maxSize = Number(groupMetaMap.get(groupId)?.size || Number.MAX_SAFE_INTEGER);
      nextSize[groupId] = Math.min(maxSize, Number(nextSize[groupId] || 0) + Number(file.size || 0));
    }
  }

  groupSelectedCountMap.value = next;
  groupSelectedSizeMap.value = nextSize;
}

function normalizeGroupedRowsResult(result: any) {
  const rows = Array.isArray(result) ? result : (Array.isArray(result?.rows) ? result.rows : []);
  const groups = Array.isArray(result?.groups)
    ? result.groups.map((group: any) => ({
        groupId: String(group.group_id ?? group.groupId ?? ''),
        label: formatGroupLabel(group.label ?? ''),
        count: Number(group.count || 0),
        size: Number(group.size || 0),
        rowIndex: Number(group.row_index ?? group.rowIndex ?? 0),
      }))
    : [];
  return {
    rows,
    groups,
    totalItemCount: Number(result?.totalItemCount ?? result?.total_item_count ?? result?.totalFileCount ?? result?.total_count ?? totalFileCount.value ?? 0),
    totalRowCount: Number(result?.total_row_count ?? result?.totalRowCount ?? rows.length),
    totalSize: Number(result?.total_size ?? result?.totalSize ?? totalFileSize.value ?? 0),
  };
}

function ensureGroupedFileAtIndex(index: number) {
  if (!groupedModeActive.value || isRealFileItem(fileList.value[index])) return true;
  const row = groupedRows.value.find((item: any) =>
    isItemRow(item) && Number(item.file_index) === Number(index)
  );
  const file = row?.file;
  if (!file) return false;

  const fileId = Number(file?.id || 0);
  const existingItem = fileList.value[index];
  fileList.value[index] = {
    ...file,
    isSelected: Boolean(existingItem?.isSelected) || (fileId > 0 && selectedFileIds.has(fileId)),
    rotate: existingItem?.rotate || file.rotate || 0,
    thumbnail: existingItem?.thumbnail || file.thumbnail,
  };
  const groupId = String(row.group_id || '');
  if (groupId && fileId) fileIdToGroupId.set(fileId, groupId);
  if (fileList.value[index]?.isSelected && fileId > 0 && !selectedFileIds.has(fileId)) {
    selectedFileIds.add(fileId);
    applySelectionDelta(fileList.value[index], 1);
  }
  return true;
}

function createGroupHeaderRow(group: any) {
  const groupId = String(group.groupId || '');
  return {
    type: 'group',
    id: `group-row-${groupId}`,
    group_id: groupId,
    label: formatGroupLabel(group.label || ''),
    count: Number(group.count || 0),
    size: Number(group.size || 0),
  };
}

function getGroupedRowIndexForFileIndex(fileIndex: number) {
  if (!groupedModeActive.value || fileIndex < 0) return fileIndex;
  let fileCursor = 0;
  for (const group of groupedTimelineGroups.value) {
    const count = Number(group.count || 0);
    if (fileIndex < fileCursor + count) {
      return Number(group.rowIndex || 0) + 1 + (fileIndex - fileCursor);
    }
    fileCursor += count;
  }
  const row = groupedRows.value.findIndex((item: any) =>
    isItemRow(item) && Number(item.file_index) === Number(fileIndex)
  );
  return row >= 0 ? row : -1;
}

function getGroupedFileIndexForRowIndex(rowIndex: number) {
  if (!groupedModeActive.value || rowIndex < 0) return rowIndex;
  let fileCursor = 0;
  for (const group of groupedTimelineGroups.value) {
    const count = Number(group.count || 0);
    const groupRowIndex = Number(group.rowIndex || 0);
    const firstItemRowIndex = groupRowIndex + 1;
    const nextGroupRowIndex = firstItemRowIndex + count;
    if (rowIndex >= groupRowIndex && rowIndex < nextGroupRowIndex) {
      return count > 0
        ? fileCursor + Math.max(0, rowIndex - firstItemRowIndex)
        : -1;
    }
    fileCursor += count;
  }

  const row = groupedRows.value[rowIndex];
  if (isItemRow(row)) return Number(row.file_index);
  if (isGroupRow(row)) {
    const nextRow = groupedRows.value[rowIndex + 1];
    return isItemRow(nextRow) ? Number(nextRow.file_index) : -1;
  }
  return -1;
}

class CopyIndexError extends Error {}

function fileTransferFailure(fallback: string, reasons: unknown[]) {
  const details = reasons.filter(reason => reason != null).map(reason => {
    const message = (reason instanceof Error ? reason.message : String(reason)).replace(/^Error:\s*/, '').trim();
    if (message.startsWith('PRIVATE_FOLDER:')) return t('media_lifecycle.file_transfer_private');
    if (message.includes('Replace would leave part of the destination media group behind')) return t('media_lifecycle.file_transfer_group');
    if (message.startsWith('Destination folder changed')) return t('media_lifecycle.file_transfer_stale');
    if (message.includes('Recovery destinations are reserved')) return t('media_lifecycle.file_transfer_recovery');
    return message.slice(0, 1000);
  }).filter(message => message && message !== fallback);
  return [fallback, ...Array.from(new Set(details)).slice(0, 3)].filter(Boolean).join('\n');
}

async function confirmLargeBatch(count: number) {
  if (count <= LARGE_BATCH_CONFIRM_THRESHOLD) return true;
  return ask(
    t('info_panel.large_batch.content', { count: count.toLocaleString() }),
    {
      title: t('info_panel.large_batch.title'),
      kind: 'warning',
      okLabel: t('info_panel.large_batch.ok'),
      cancelLabel: t('msgbox.cancel'),
    },
  );
}

async function runWithConcurrency<T, R>(
  items: T[],
  worker: (item: T) => Promise<R>,
  concurrency = FILE_OPERATION_CONCURRENCY,
): Promise<PromiseSettledResult<R>[]> {
  const results: PromiseSettledResult<R>[] = new Array(items.length);
  let nextIndex = 0;
  const workerCount = Math.min(Math.max(1, concurrency), items.length);

  await Promise.all(Array.from({ length: workerCount }, async () => {
    while (nextIndex < items.length) {
      const index = nextIndex++;
      try {
        results[index] = { status: 'fulfilled', value: await worker(items[index]) };
      } catch (reason) {
        results[index] = { status: 'rejected', reason };
      }
    }
  }));
  return results;
}

async function runWithKeyedConcurrency<T, R>(
  items: T[],
  getKey: (item: T) => string,
  worker: (item: T) => Promise<R>,
  concurrency = FILE_OPERATION_CONCURRENCY,
): Promise<PromiseSettledResult<R>[]> {
  const groups = new Map<string, Array<{ item: T; index: number }>>();
  items.forEach((item, index) => {
    const key = getKey(item);
    const group = groups.get(key);
    if (group) {
      group.push({ item, index });
    } else {
      groups.set(key, [{ item, index }]);
    }
  });

  const results: PromiseSettledResult<R>[] = new Array(items.length);
  await runWithConcurrency(Array.from(groups.values()), async group => {
    for (const { item, index } of group) {
      try {
        results[index] = { status: 'fulfilled', value: await worker(item) };
      } catch (reason) {
        results[index] = { status: 'rejected', reason };
      }
    }
  }, concurrency);
  return results;
}

// quick view
const showQuickView = ref(false);
const quickViewMediaRef = ref<any>(null);
const previewViewports = ref<{ quick: any; filmstrip: any }>({ quick: null, filmstrip: null });
const infoNavigatorVisible = ref(false);
const infoNavigatorViewport = computed(() => {
  const viewport = showQuickView.value ? previewViewports.value.quick
    : showFilmstripLayout.value ? previewViewports.value.filmstrip : null;
  return viewport && viewport.fileId === fileList.value[selectedItemIndex.value]?.id
    && viewport.sourceWidth > 0 && viewport.sourceHeight > 0 ? viewport : null;
});
function navigateInfoPreview(point: { normX: number; normY: number }) {
  getActivePreviewMediaRef()?.navigateImage?.(point);
}

const quickViewZoomFit = ref(true);
const quickViewHoverLeft = ref(false);
const quickViewHoverRight = ref(false);

function handleQuickViewMouseMove(event: MouseEvent) {
  const target = event.currentTarget as HTMLElement | null;
  if (!target) return;
  const rect = target.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return;
  const x = event.clientX - rect.left;
  quickViewHoverLeft.value = x >= 0 && x < rect.width * 0.1;
  quickViewHoverRight.value = x <= rect.width && x > rect.width * 0.9;
}

function handleQuickViewMouseLeave() {
  quickViewHoverLeft.value = false;
  quickViewHoverRight.value = false;
}

function getActivePreviewMode(): 'quick-view' | 'filmstrip' | 'none' {
  if (showQuickView.value) return 'quick-view';
  if (isFilmstripView.value) return 'filmstrip';
  return 'none';
}

function getActivePreviewMediaRef() {
  if (showQuickView.value) return quickViewMediaRef.value;
  if (isFilmstripView.value) return filmStripMediaRef.value;
  return null;
}

async function prepareSettingsOpen() {
  const preview = getActivePreviewMediaRef();
  await preview?.exitPreviewFullScreen?.();
  if (preview?.isFullScreen) return false;
  stopSlideShow();
  return true;
}

async function seekMediaAiVideo(seconds: number) {
  if (!isFilmstripView.value) showQuickView.value = true;
  await nextTick();
  getActivePreviewMediaRef()?.seekVideo?.(seconds);
}

function getCurrentPreviewImageSrc() {
  const viewer = getActivePreviewMediaRef();
  return viewer?.getCurrentImageSrc?.() || '';
}

function clearPreviewPreloadCache(filePath?: string) {
  const viewer = getActivePreviewMediaRef();
  viewer?.clearPreloadCache?.(filePath);
}

// film strip view
const filmStripMediaRef = ref<any>(null);
const filmStripZoomFit = ref(true);

function closeQuickPreview() {
  showQuickView.value = false;
  stopSlideShow();
}

function closeFilmstripFullScreen() {
  stopSlideShow();
  void filmStripMediaRef.value?.exitPreviewFullScreen();
}

function setPreviewViewBackground(value: number) {
  config.setViewBackground(value);
  void tauriEmit('settings-viewBackground-changed', config.settings.viewBackground);
}

// toolbar state for MediaViewer
const imageScale = ref(1);
const imageDisplayScale = ref(1);
const imageMinScale = ref(0);
const imageMaxScale = ref(10);
const isSlideShow = ref(false);

function getNextImagePath(index: number): string {
  const target = fileList.value[index + 1];
  return target && !target.isPlaceholder && target.file_type === 1 ? target.file_path : '';
}

// Request ID tracking to prevent race conditions during async content updates
let currentContentRequestId = 0;

const onScale = (event: any) => {
  imageScale.value = event.scale;
  imageDisplayScale.value = event.displayScale ?? event.scale;
  imageMinScale.value = event.minScale;
  imageMaxScale.value = event.maxScale;
};

const videoRef = ref<HTMLVideoElement | null>(null);             // preview video reference

// info panel splitter
const isDraggingInfoPanel = ref(false);
const rightPanelDragStartX = ref(0);
const rightPanelDragStartWidth = ref(0);

// message box
const showRenameMsgbox = ref(false);  // show rename message box
const renamingFileName = ref<{name?: string, ext?: string}>({}); // extract the file name to {name, ext}

const showMoveTo = ref(false);
type FileConflictPolicy = 'skip' | 'keep_both' | 'replace';
const fileConflictDialog = ref({
  show: false,
  name: '',
  destination: '',
  showApplyAll: false,
  allowReplace: true,
});
let fileConflictResolver: ((result: { policy: FileConflictPolicy; applyAll: boolean }) => void) | null = null;
const hideBusy=ref(false);const showHideMsgbox=ref(false);const pendingHide=ref<{libraryId:string;ids:number[]}|null>(null);
async function confirmHide(){
  const request=pendingHide.value;if(!request||hideBusy.value)return;hideBusy.value=true;
  try{await setMediaHidden(request.libraryId,request.ids,true);}catch(error){if(request.libraryId===libConfig._libraryId)toast.error(String(error));}
  finally{hideBusy.value=false;showHideMsgbox.value=false;pendingHide.value=null;}
}
watch(()=>libConfig._libraryId,()=>{showHideMsgbox.value=false;pendingHide.value=null;});
const showTrashMsgbox = ref(false);
const isTrashDeleting = ref(false);
const showTrashFailedMsgbox = ref(false);
const showExternalOpenWarningMsgbox = ref(false);
const showExternalAppsDialog = ref(false);
const pendingExternalOpen = ref<{ paths: string[]; appPath: string } | null>(null);
const permanentDeleteChecked = ref(false);
const deletePermanently = ref(false);
const pendingTrashFailedItems = ref<any[]>([]);
const pendingTrashFailedDedupGroupKey = ref('');
const pendingTrashFailedOtherFailureCount = ref(0);
const dedupReclaimBytes = ref(0);
const dedupTrashGroupKey = ref('');
const dedupDeleteFileIds = ref<number[]>([]);
const dedupBulkFileCount = ref(0);
const dedupPaneRef = ref<InstanceType<typeof DedupPane> | null>(null);
const dedupStatuses = ref<Record<number, 'keep' | 'dup'>>({});
const showCommentMsgbox = ref(false);
const commentInputText = computed(() => {
  if (selectMode.value) {
    const selectedItems = getActionableSelectedItems();
    if (selectedItems.length === 0) return '';
    const firstComment = selectedItems[0]?.comments ?? '';
    return selectedItems.every(item => (item.comments ?? '') === firstComment) ? firstComment : '';
  }
  return fileList.value[selectedItemIndex.value]?.comments ?? '';
});
const errorMessage = ref('');

// Unsaved changes confirmation
const showUnsavedChangesMsgbox = ref(false);
const pendingAction = ref<(() => void) | null>(null);
const fileInfoRef = ref<any>(null);
const isDedupPanelOpen = computed(() => !selectionPanelOpen.value && config.rightPanel.show && config.rightPanel.mode === 'dedup');
const isInfoPanelOpen = computed(() => !selectionPanelOpen.value && config.rightPanel.show && config.rightPanel.mode === 'info');
const rightPanelContent = computed<'selection' | 'dedup' | 'info' | null>(() => {
  if (selectMode.value && selectionPanelOpen.value) return 'selection';
  if (!config.rightPanel.show) return null;
  return config.rightPanel.mode === 'dedup' ? 'dedup' : 'info';
});
const renderedRightPanelContent = ref(rightPanelContent.value);
watch(rightPanelContent, (content) => {
  if (content) renderedRightPanelContent.value = content;
});

const activeRightPanelWidth = computed(() => Number(config.rightPanel.width || 360));
const shouldShowRightPanel = computed(() => config.rightPanel.show || (selectMode.value && selectionPanelOpen.value));
const rightPanelMounted = ref(shouldShowRightPanel.value);
const rightPanelVisualVisible = ref(shouldShowRightPanel.value);
const rightPanelLayoutVisible = ref(shouldShowRightPanel.value);
let rightPanelAnimationTimer: ReturnType<typeof setTimeout> | null = null;
let rightPanelAnimationVersion = 0;

async function refreshGridLayoutPreservingScroll() {
  const scrollTop = gridViewRef.value?.getScrollTop();
  gridViewRef.value?.refreshLayout?.();
  await nextTick();
  if (scrollTop !== undefined) gridViewRef.value?.scrollToPosition?.(scrollTop);
}

async function commitRightPanelLayout(visible: boolean) {
  rightPanelLayoutVisible.value = visible;
  await nextTick();
  await refreshGridLayoutPreservingScroll();
}

function clearRightPanelAnimationTimer() {
  if (rightPanelAnimationTimer) {
    clearTimeout(rightPanelAnimationTimer);
    rightPanelAnimationTimer = null;
  }
}

watch(shouldShowRightPanel, async (visible) => {
  clearRightPanelAnimationTimer();
  const animationVersion = ++rightPanelAnimationVersion;

  if (visible) {
    rightPanelMounted.value = true;
    rightPanelLayoutVisible.value = true;
    await nextTick();
    if (animationVersion !== rightPanelAnimationVersion) return;
    rightPanelVisualVisible.value = true;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (animationVersion === rightPanelAnimationVersion) {
          void refreshGridLayoutPreservingScroll();
        }
      });
    });
    return;
  }

  rightPanelVisualVisible.value = false;
  rightPanelAnimationTimer = setTimeout(() => {
    if (animationVersion !== rightPanelAnimationVersion) return;
    rightPanelAnimationTimer = null;
    void commitRightPanelLayout(false).then(() => {
      if (animationVersion !== rightPanelAnimationVersion) return;
      rightPanelMounted.value = false;
    });
  }, PANEL_ANIMATION_MS);
});

function getRightPanelMaxWidth() {
  const mainWindowWidth = window.innerWidth;
  return Math.max(RIGHT_PANEL_MIN_WIDTH, Math.floor(mainWindowWidth / 3));
}

function clampRightPanelWidth(width: number) {
  return Math.round(Math.min(Math.max(width, RIGHT_PANEL_MIN_WIDTH), getRightPanelMaxWidth()));
}

function migrateRightPanelWidthToPixels() {
  if (!contentViewDiv.value) return;
  const layoutWidth = contentViewDiv.value.clientWidth;
  if (layoutWidth <= 0) return;

  const rawWidth = Number(config.rightPanel.width || 0);
  if (rawWidth <= 0) {
    config.rightPanel.width = clampRightPanelWidth(layoutWidth * 0.3);
    return;
  }

  // Backward compatibility: old config persisted width in percent (20-80).
  if (rawWidth <= 100) {
    config.rightPanel.width = clampRightPanelWidth((rawWidth / 100) * layoutWidth);
    return;
  }

  config.rightPanel.width = clampRightPanelWidth(rawWidth);
}

function handleWindowResize() {
  config.rightPanel.width = clampRightPanelWidth(Number(config.rightPanel.width || 360));
}

const confirmSave = async () => {
  showUnsavedChangesMsgbox.value = false;
  if (fileInfoRef.value) {
    const success = await fileInfoRef.value.quickSave();
    if (success && pendingAction.value) {
      pendingAction.value();
      pendingAction.value = null;
    }
  }
};

const confirmDiscard = () => {
  showUnsavedChangesMsgbox.value = false;
  // Clear active adjustments in store to "discard" the changes
  uiStore.clearActiveAdjustments();
  if (pendingAction.value) {
    pendingAction.value();
    pendingAction.value = null;
  }
};

const cancelDiscard = () => {
  showUnsavedChangesMsgbox.value = false;
  pendingAction.value = null;
};

// Check if current file has unsaved changes
const hasUnsavedChanges = computed(() => {
  if (!isInfoPanelOpen.value) return false;
  const currentFile = fileList.value[selectedItemIndex.value];
  return uiStore.hasActiveChanges(currentFile);
});

const checkUnsavedChanges = (next: () => void) => {
  const action = () => {
    if (!pendingChanges.hasPending('media-title')) { next(); return; }
    const library = libConfig._libraryId;
    void pendingChanges.prepare('media-title').then(token => {
      if (token && pendingChanges.valid(token) && library === libConfig._libraryId) next();
    });
  };
  if (hasUnsavedChanges.value) {
    pendingAction.value = action;
    showUnsavedChangesMsgbox.value = true;
  } else {
    action();
  }
};

// Open a temporary view filtered by the clicked metadata (camera/lens/location).
// Like "find related photos", the back button returns to the previous view.
function enterMetadataTempView(
  mode: 'camera' | 'lens' | 'location',
  title: string,
  params: Record<string, any>,
) {
  if (tempViewMode.value === 'none') {
    backupState.value = createViewBackup();
  }
  invalidateThumbnailRequests();
  tempViewMode.value = mode;
  showQuickView.value = false;
  contentTitle.value = title;
  const requestId = ++currentContentRequestId;
  showLoadingContent(requestId);
  scrollPosition.value = 0;
  selectedItemIndex.value = 0;
  if (gridViewRef.value) {
    gridViewRef.value.scrollToPosition(0);
  }
  getFileList(params, requestId);
}

function handleNavigateMetadata(payload: { type: string; [key: string]: any }) {
  switch (payload?.type) {
    case 'camera': {
      const make = payload.make || '';
      const model = payload.model || '';
      enterMetadataTempView('camera', [make, model].filter(Boolean).join(' '), {
        make, model, searchFileType: 0,
      });
      break;
    }
    case 'lens': {
      enterMetadataTempView('lens', payload.lensModel || payload.lensMake || '', {
        lensMake: payload.lensMake || '', lensModel: payload.lensModel || '', searchFileType: 0,
      });
      break;
    }
    case 'location': {
      const title = payload.label || '';
      enterMetadataTempView('location', title, {
        locationNodeId: payload.nodeId || 0, searchFileType: 0,
      });
      break;
    }
  }
}

async function removeFileFromCurrentCollection(index: number) {
  const file = fileList.value[index];
  if (!file || !currentCollectionId.value) return;
  await removeFilesFromCollection(currentCollectionId.value, [file.id]);
  if (groupedModeActive.value) {
    await updateContent(true);
  } else {
    fileList.value.splice(index, 1);
    totalFileCount.value = fileList.value.length;
    totalFileSize.value -= file.size || 0;
    if (selectedItemIndex.value >= fileList.value.length) {
      selectedItemIndex.value = fileList.value.length - 1;
    }
  }
  void tauriEmit('collection-files-dropped', { collectionId: currentCollectionId.value });
  toast.success(t('collection.removed_toast', { count: 1 }));
}

async function removeSelectedFromCollection() {
  if (!selectMode.value || selectedCount.value === 0 || !currentCollectionId.value) return;
  const selectedItems = getActionableSelectedItems();
  const fileIds = selectedItems.map((item: any) => Number(item.id)).filter((id: number) => id > 0);
  if (fileIds.length === 0) return;
  await removeFilesFromCollection(currentCollectionId.value, fileIds);
  if (groupedModeActive.value) {
    await updateContent(true);
  } else {
    const removedIdSet = new Set(fileIds);
    fileList.value = fileList.value.filter(f => !removedIdSet.has(f.id));
    totalFileCount.value = fileList.value.length;
    totalFileSize.value = fileList.value.reduce((total, file) => total + file.size, 0);
    selectedItemIndex.value = fileList.value.length > 0 ? Math.min(selectedItemIndex.value, fileList.value.length - 1) : -1;
    selectedCount.value = Math.max(0, selectedCount.value - fileIds.length);
    selectedSize.value = Math.max(0, selectedSize.value - selectedItems.reduce((total, item) => total + Number(item.size || 0), 0));
  }
  void tauriEmit('collection-files-dropped', { collectionId: currentCollectionId.value });
  toast.success(t('collection.removed_toast', { count: fileIds.length }));
}

const openTrashMsgbox = (reclaimBytes = 0, groupKey = '', fileIds: number[] = []) => {
  if (fileIds.length === 0 && currentOriginalsUnavailable()) return;
  if (selectMode.value && selectedCount.value === 0 && fileIds.length === 0) return;
  dedupReclaimBytes.value = Math.max(0, reclaimBytes);
  dedupTrashGroupKey.value = groupKey || '';
  dedupDeleteFileIds.value = Array.isArray(fileIds) ? [...new Set(fileIds)] : [];
  deletePermanently.value = false;
  showTrashMsgbox.value = true;
};

const openAllDuplicatesTrashMsgbox = (fileCount: number, reclaimBytes: number) => {
  if (fileCount <= 0) return;
  dedupReclaimBytes.value = Math.max(0, reclaimBytes);
  dedupBulkFileCount.value = fileCount;
  dedupTrashGroupKey.value = 'all';
  dedupDeleteFileIds.value = [];
  deletePermanently.value = false;
  showTrashMsgbox.value = true;
};

const closeTrashMsgbox = () => {
  showTrashMsgbox.value = false;
  dedupReclaimBytes.value = 0;
  dedupTrashGroupKey.value = '';
  dedupDeleteFileIds.value = [];
  dedupBulkFileCount.value = 0;
};

const isDedupBulkTrash = computed(() => dedupBulkFileCount.value > 0);
const isDedupTrash = computed(() => dedupDeleteFileIds.value.length > 0 || isDedupBulkTrash.value);
const rawJpegCompanionDeleteCount = computed(() => {
  if (!config.settings.groupRawJpegPairs) return 0;
  const items = isDedupTrash.value
    ? dedupDeleteFileIds.value.map(id => fileList.value.find(file => Number(file?.id) === Number(id))).filter(Boolean)
    : selectMode.value
      ? getActionableSelectedItems()
      : [fileList.value[selectedItemIndex.value]].filter(Boolean);
  return items.filter((file: any) => file.media_subtype === 'raw_jpeg_pair').length;
});

const trashMsgboxTitle = computed(() => {
  if (deletePermanently.value) {
    return localeMsg.value.msgbox.permanent_delete.title;
  }
  return localeMsg.value.msgbox.move_to_trash.title;
});

const trashMsgboxOkText = computed(() => {
  if (deletePermanently.value) {
    return localeMsg.value.msgbox.permanent_delete.ok;
  }
  return localeMsg.value.msgbox.move_to_trash.ok;
});

const trashMsgboxMessage = computed(() => {
  const deleteCount = isDedupBulkTrash.value
    ? dedupBulkFileCount.value
    : (isDedupTrash.value ? dedupDeleteFileIds.value.length : selectedCount.value);
  const base = isDedupBulkTrash.value
    ? t(
        deletePermanently.value
          ? 'msgbox.permanent_delete.removable_files_content'
          : 'msgbox.move_to_trash.removable_files_content',
        { count: deleteCount.toLocaleString() },
      )
    : deletePermanently.value
    ? ((isDedupTrash.value || selectMode.value)
        ? localeMsg.value.msgbox.permanent_delete.files_content.replace('{count}', deleteCount.toLocaleString())
        : localeMsg.value.msgbox.permanent_delete.file_content.replace('{file}', fileList.value[selectedItemIndex.value]?.name || ''))
    : ((isDedupTrash.value || selectMode.value)
        ? localeMsg.value.msgbox.move_to_trash.files_content.replace('{count}', deleteCount.toLocaleString())
        : localeMsg.value.msgbox.move_to_trash.file_content.replace('{file}', fileList.value[selectedItemIndex.value]?.name || ''));
  const companionNotice = rawJpegCompanionDeleteCount.value > 0
    ? `\n${localeMsg.value.msgbox.move_to_trash.raw_jpeg_companions_content.replace('{count}', rawJpegCompanionDeleteCount.value.toLocaleString())}`
    : '';
  if (dedupReclaimBytes.value <= 0 || !(isDedupTrash.value || selectMode.value)) return `${base}${companionNotice}`;
  return `${base}${companionNotice}\n${localeMsg.value.info_panel.dedup.reclaimable_size}: ${formatFileSize(dedupReclaimBytes.value)}`;
});

const trashFailedMsgboxMessage = computed(() => {
  const deleteCount = pendingTrashFailedItems.value.length > 0
    ? pendingTrashFailedItems.value.length
    : (isDedupTrash.value ? dedupDeleteFileIds.value.length : selectedCount.value);
  if (deleteCount > 1 || isDedupTrash.value || selectMode.value) {
    return localeMsg.value.msgbox.trash_failed.files_content.replace('{count}', deleteCount.toLocaleString());
  }
  return localeMsg.value.msgbox.trash_failed.file_content.replace(
    '{file}',
    pendingTrashFailedItems.value[0]?.name || fileList.value[selectedItemIndex.value]?.name || '',
  );
});

const confirmTrashFailedPermanentDelete = async () => {
  const previousDeletePermanently = deletePermanently.value;
  const previousPermanentDeleteChecked = permanentDeleteChecked.value;
  const retryItems = [...pendingTrashFailedItems.value];
  const retryDedupGroupKey = pendingTrashFailedDedupGroupKey.value;
  showTrashFailedMsgbox.value = false;
  deletePermanently.value = true;
  try {
    if (retryItems.length > 0 && retryDedupGroupKey) {
      dedupTrashGroupKey.value = retryDedupGroupKey;
      dedupDeleteFileIds.value = retryItems.map(item => Number(item.id)).filter(id => id > 0);
    }
    await onTrashFile(retryItems);
  } finally {
    pendingTrashFailedItems.value = [];
    pendingTrashFailedDedupGroupKey.value = '';
    pendingTrashFailedOtherFailureCount.value = 0;
    deletePermanently.value = previousDeletePermanently;
    permanentDeleteChecked.value = previousPermanentDeleteChecked;
  }
};

const getFileItemsByIds = (ids: number[]) => {
  const targetIdSet = new Set(ids.map(id => Number(id)).filter(id => id > 0));
  return fileList.value.filter(file => targetIdSet.has(Number(file?.id)));
};

const cancelTrashFailedMsgbox = () => {
  showTrashFailedMsgbox.value = false;
  pendingTrashFailedItems.value = [];
  pendingTrashFailedDedupGroupKey.value = '';
  pendingTrashFailedOtherFailureCount.value = 0;
  closeTrashMsgbox();
};

// tagging dialog
const showTaggingDialog = ref(false);
const fileIdsToTag = ref<number[]>([]);
const showAddToCollectionDialog = ref(false);
const fileIdsToAddToCollection = ref<number[]>([]);

// grid view
const gridScrollContainerRef = ref<HTMLElement | null>(null);
const gridViewRef = ref<any>(null);

const scrollPosition = ref(0);    // scrollbar position (item index)

const containerHeight = ref(0);   // container height
const containerWidth = ref(0);    // container width
const layoutContentHeight = ref(0); // reported content height from GridView
const layoutVersion = ref(0);     // version to force layout update
let layoutRefreshTimer: ReturnType<typeof setTimeout> | null = null;
const isGeometryGridStyle = computed(() => config.settings.grid.style === 2 || config.settings.grid.style === 3);
const usesGeometryNavigation = computed(() =>
  (groupedModeActive.value && !isFilmstripView.value) ||
  config.settings.grid.style === 2 ||
  (!isFilmstripView.value && config.settings.grid.style === 3)
);

function scheduleLayoutRefresh() {
  if (!isGeometryGridStyle.value) return;
  if (layoutRefreshTimer) return;
  layoutRefreshTimer = setTimeout(() => {
    layoutVersion.value++;
    layoutRefreshTimer = null;
  }, 120);
}
const gap = 8;                    // Gap between items (must match GridView)
const gridSizeProfile = computed(() => thumbnailProfile(config.settings.thumbnailSize));
const gridSize = computed(() =>
  gridSizeFromPosition(config.settings.grid.sizePosition, config.settings.thumbnailSize),
);
const gridSizePosition = computed({
  get: () => Math.round(normalizeGridSizePosition(config.settings.grid.sizePosition) * 100),
  set: (value) => {
    config.settings.grid.sizePosition = normalizeGridSizePosition(Number(value) / 100);
  },
});

function setGridSize(value: unknown) {
  const profile = gridSizeProfile.value;
  const size = Math.min(profile.maxGridSize, Math.max(profile.minGridSize, Number(value) || profile.minGridSize));
  config.settings.grid.sizePosition = gridSizePositionFromGridSize(size, config.settings.thumbnailSize);
}

const itemWidth = computed(() => {
  if (config.settings.grid.style === 0) {
    return gridSize.value + 20; // size + padding/border/gap(20)
  } else if (config.settings.grid.style === 1) {
    return gridSize.value;
  } else if (isGeometryGridStyle.value) {
    return gridSize.value; // Approximation for geometry layouts
  }
  return 0;
});

const itemSize = computed(() => {
  if (config.settings.grid.style === 0) {
    let labelHeight = 0
    if (config.settings.grid.labelPrimary > 0 ) labelHeight += 16;      // height of text-sm
    if (config.settings.grid.labelSecondary > 0 ) labelHeight += 16;    // height of text-xs
    return gridSize.value + 20 + labelHeight; // size + padding/border/gap(20) + labels
  } else if (config.settings.grid.style === 1) {
    return itemWidth.value + gap / 2;
  } else if (isGeometryGridStyle.value) {
    return gridSize.value;
  }
  return 0;
});

const columnCount = computed(() => {
  if (containerWidth.value <= 0 || itemWidth.value <= 0) return 4;
  return Math.max(1, Math.floor(containerWidth.value / itemWidth.value));
});

const visibleItemCount = computed(() => {
  if (containerHeight.value <= 0 || itemSize.value <= 0) return 20;
  const rows = Math.floor(containerHeight.value / itemSize.value);
  return rows * columnCount.value;
});

const timelineData = ref<any[]>([]);  // timeline markers for scrollbar
const groupTimelineData = computed(() => {
  if (!groupedModeActive.value) return [];
  if (groupedTimelineGroups.value.length > 0) {
    return groupedTimelineGroups.value.map((group: any) => ({
      label: group.label || '',
      position: Number(group.rowIndex || 0),
    }));
  }
  return groupedRows.value
    .map((row: any, index: number) => isGroupRow(row) && !row.isPlaceholder
      ? {
          label: row.label || '',
          position: index,
        }
      : null)
    .filter(Boolean);
});
const scrollbarTotal = computed(() => groupedModeActive.value ? totalRowCount.value : totalFileCount.value);
const scrollbarPageSize = computed(() => Math.max(1, Math.min(visibleItemCount.value, scrollbarTotal.value || visibleItemCount.value)));
const scrollbarMarkers = computed(() => {
  if (groupedModeActive.value) return groupTimelineData.value;
  if (isSearchLikeView.value) return [];
  return timelineData.value;
});
const scrollbarSelectedIndex = computed(() => (
  groupedModeActive.value
    ? getGroupedRowIndexForFileIndex(selectedItemIndex.value)
    : selectedItemIndex.value
));

const toast = useToast();
const shortcutPlatform: ShortcutPlatform = isMac ? 'mac' : (isLinux ? 'linux' : 'windows');
const pendingFolderSyncs = new Map<number, Promise<any>>();
let lastSyncedAlbumFolder = '';
const shortcut = (actionId: ShortcutActionId) => getShortcutLabel(actionId, shortcutPlatform);
const ratingActions: Array<{ actionId: ShortcutActionId; rating: number }> = [
  { actionId: 'meta.rating.clear', rating: 0 },
  { actionId: 'meta.rating.one', rating: 1 },
  { actionId: 'meta.rating.two', rating: 2 },
  { actionId: 'meta.rating.three', rating: 3 },
  { actionId: 'meta.rating.four', rating: 4 },
  { actionId: 'meta.rating.five', rating: 5 },
];
const cullingActions: Array<{ actionId: ShortcutActionId; cullingFlag: number }> = [
  { actionId: 'meta.culling.pick', cullingFlag: 1 },
  { actionId: 'meta.culling.reject', cullingFlag: 2 },
  { actionId: 'meta.culling.unreviewed', cullingFlag: 0 },
];

function getMatchedRating(event: KeyboardEvent) {
  const match = ratingActions.find(({ actionId }) => matchesShortcut(actionId, event, shortcutPlatform));
  return match ? match.rating : null;
}

function getMatchedCulling(event: KeyboardEvent) {
  const match = cullingActions.find(({ actionId }) => matchesShortcut(actionId, event, shortcutPlatform));
  return match ? match.cullingFlag : null;
}

function getMatchedViewBackground(event: KeyboardEvent): number | null {
  const match = VIEW_BACKGROUND_SHORTCUTS.find(({ actionId }) => matchesShortcut(actionId, event, shortcutPlatform));
  return match?.value ?? null;
}

// Drag-drop file import
const isDragOver = ref(false);
const dragOverCount = ref(0);
const showDropWarning = ref(false);
const isContentInternalDrag = ref(false);
const draggedFileIds = ref(new Set<number>());
const isPastingClipboard = ref(false);
const acceptDrops = computed(() =>
  tempViewMode.value === 'none'
  && config.main.sidebarIndex === SIDEBAR.ALBUM
  && libConfig.album.id > 0
);

// DOM drop handler references (Windows/Linux — for cleanup in onBeforeUnmount)
let domDragEnter: ((e: DragEvent) => void) | null = null;
let domDragLeave: ((e: DragEvent) => void) | null = null;
let domDragOver: ((e: DragEvent) => void) | null = null;
let domDragEnd: ((e: DragEvent) => void) | null = null;
let domDrop: ((e: DragEvent) => void) | null = null;
let dragGhost: HTMLElement | null = null;
let dragGhostAction: HTMLElement | null = null;
let pointerDropTarget: HTMLElement | null = null;
let nativeDragFiles: Promise<any[] | null> | null = null;
let nativeDragPreview: number[] = [];
let pointerDragUsesSelection = false;
let pointerDragFiles: Array<{
  id: number;
  file_path: string;
  folder_id: number;
  album_id: number;
}> | null = null;
let dragGhostHotspotX = 0;
let dragGhostHotspotY = 0;

function hasExternalDomDrop(event: DragEvent) {
  if (isNativeFileDragActive()) return false;
  return hasExternalDragIntent(event)
    || getExternalDropUris(event.dataTransfer).some(uri => fileUrlToPath(uri) || /^https?:\/\//.test(uri));
}

function hasExternalDragIntent(event: DragEvent) {
  if (isContentInternalDrag.value || isNativeFileDragActive()) return false;
  return hasExternalDropIntent(event.dataTransfer);
}

watch(externalDragKind, kind => {
  if (kind !== 'files') clearDropOverlay();
});

function clearDropOverlay() {
  dragOverCount.value = 0;
  isDragOver.value = false;
}

function isInternalReorderActive() {
  return uiStore.isInputActive('ManageLibraries')
    || uiStore.isInputActive('AlbumListDrag')
    || uiStore.isInputActive('CollectionTrayDrag');
}

async function resolveAlbumImportDestination(albumId: number, folderPath?: string) {
  if (albumId <= 0) return null;
  const album = await getAlbum(albumId);
  const destinationPath = folderPath || album?.path;
  if (!destinationPath) return null;

  const folder = await selectFolder(albumId, destinationPath);
  return folder?.id
    ? { albumId, folderId: Number(folder.id), folderPath: String(folder.path) }
    : null;
}

async function resolveCurrentAlbumImportDestination() {
  const albumId = Number(libConfig.album.id || 0);
  if (config.main.sidebarIndex !== SIDEBAR.ALBUM || albumId <= 0) return null;

  const folderPath = !libConfig.album.selected && libConfig.album.folderPath
    ? libConfig.album.folderPath
    : undefined;
  return resolveAlbumImportDestination(albumId, folderPath);
}

async function refreshImportedFiles(albumId: number, folderPath: string) {
  await refreshFolderSidebarCounts([folderPath]);
  await refreshAffectedAlbums([albumId]);
  await refreshLibraryTotalCount();
  await updateContent();
}

let fileImportDisposed = false;
const isImportingFiles = ref(false);
const fileImportVisible = ref(false);
const fileImportTarget = ref('');
const fileImportProcessed = ref(0);
const fileImportTotal = ref(0);
const fileImportSuccess = ref(0);
const fileImportErrors = ref<string[]>([]);

async function importFilesToFolder(target: { albumId: number; folderPath: string }) {
  if (isImportingFiles.value) return;
  const libraryId = libConfig._libraryId;
  const isCurrent = () => !fileImportDisposed && libConfig._libraryId === libraryId;
  isImportingFiles.value = true;
  let progressTimer: ReturnType<typeof setTimeout> | undefined;
  try {
    const formats = await getSupportedFormatExtensions();
    const extensions = [...formats.image, ...formats.raw, ...formats.video];
    if (!extensions.length) throw new Error(t('import_files.formats_failed'));
    const selected = await openDialog({
      title: t('import_files.title'),
      multiple: true,
      directory: false,
      filters: [{ name: t('import_files.media'), extensions }],
    });
    if (!isCurrent() || !selected || (Array.isArray(selected) && !selected.length)) return;
    const paths = [...new Set(Array.isArray(selected) ? selected : [selected])];
    if (!await isDirectoryAccessible(target.folderPath)) throw new Error(t('import_files.destination_unavailable'));
    const destination = await resolveAlbumImportDestination(target.albumId, target.folderPath);
    if (!isCurrent()) return;
    if (!destination) throw new Error(t('import_files.destination_unavailable'));
    fileImportTarget.value = destination.folderPath;
    fileImportProcessed.value = 0;
    fileImportTotal.value = paths.length;
    fileImportSuccess.value = 0;
    fileImportErrors.value = [];
    fileImportVisible.value = false;
    progressTimer = setTimeout(() => { fileImportVisible.value = true; }, 700);
    for (const path of paths) {
      if (!isCurrent()) return;
      try {
        const file = await importFile(path, destination.folderId, destination.folderPath, true, libraryId);
        if (!file) throw new Error(t('import_files.copy_failed'));
        fileImportSuccess.value++;
      } catch (error) {
        fileImportErrors.value.push(`${path}\n${String(error)}`);
      }
      fileImportProcessed.value++;
    }
    clearTimeout(progressTimer);
    if (!isCurrent()) return;
    if (fileImportSuccess.value > 0) {
      await refreshFolderSidebarCounts([destination.folderPath]);
      if (!isCurrent()) return;
      await refreshAffectedAlbums([destination.albumId], true);
      if (!isCurrent()) return;
      await refreshLibraryTotalCount();
      if (!isCurrent()) return;
      await refreshImportedAlbumContent(destination.albumId);
    }
    if (!isCurrent()) return;
    fileImportVisible.value = fileImportErrors.value.length > 0;
    if (fileImportErrors.value.length) {
      toast.warning(t('import_files.summary', { imported: fileImportSuccess.value, failed: fileImportErrors.value.length }));
    } else {
      toast.success(t('msgbox.drop_import.success', { count: fileImportSuccess.value }));
    }
  } catch (error) {
    if (isCurrent()) toast.error(String(error));
  } finally {
    clearTimeout(progressTimer);
    isImportingFiles.value = false;
  }
}

async function pasteClipboardImage(target?: { albumId: number; folderPath?: string }) {
  if (isPastingClipboard.value) return;
  if (!target && !acceptDrops.value) {
    showDropWarning.value = true;
    return;
  }

  const destination = target
    ? await resolveAlbumImportDestination(Number(target.albumId || 0), target.folderPath)
    : await resolveCurrentAlbumImportDestination();
  if (!destination) {
    showDropWarning.value = true;
    return;
  }

  isPastingClipboard.value = true;
  try {
    const files = await importClipboard(destination.folderId, destination.folderPath);
    if (!Array.isArray(files) || files.length === 0) {
      toast.warning(t('msgbox.drop_import.no_clipboard_image'));
      return;
    }
    await refreshImportedFiles(destination.albumId, destination.folderPath);
    toast.success(t('msgbox.drop_import.success', { count: files.length }));
  } catch (error) {
    console.error('Failed to import clipboard image:', error);
    toast.warning(t('msgbox.drop_import.no_clipboard_image'));
  } finally {
    isPastingClipboard.value = false;
  }
}

function removeDragGhost() {
  if (dragGhostAction?.firstElementChild) {
    render(null, dragGhostAction.firstElementChild as HTMLElement);
  }
  dragGhost?.remove();
  dragGhost = null;
  dragGhostAction = null;
  setPointerDropTarget(null);
  document.removeEventListener('pointermove', updateContentDragPosition);
  document.documentElement.removeEventListener('mouseleave', onContentWindowLeave);
  document.removeEventListener('keydown', updateDragGhostModifier);
  document.removeEventListener('keyup', updateDragGhostModifier);
}

function setPointerDropTarget(target: HTMLElement | null) {
  if (pointerDropTarget === target) return;
  pointerDropTarget?.classList.remove('!bg-primary/10', '!border-primary/60', 'ring-2', 'ring-inset', 'ring-primary/40');
  pointerDropTarget = target;
  if (pointerDropTarget?.hasAttribute('data-media-chat-drop')) {
    pointerDropTarget.classList.add('!border-primary/60', 'ring-2', 'ring-inset', 'ring-primary/40');
  } else {
    pointerDropTarget?.classList.add('!bg-primary/10', '!border-primary/60');
  }
}

function isCopyDragModifier(event: Pick<MouseEvent, 'altKey' | 'ctrlKey'>) {
  return isMac ? event.altKey : event.ctrlKey;
}

function updateDragGhostAction(event: Pick<MouseEvent, 'altKey' | 'ctrlKey'>) {
  if (!dragGhostAction) return;
  const copy = isCopyDragModifier(event);
  const showLabel = !!pointerDropTarget;
  const isCollectionDropTarget = !!pointerDropTarget?.dataset.collectionDropId || pointerDropTarget?.dataset.collectionDropNew === 'true';
  const isChatDropTarget = pointerDropTarget?.hasAttribute('data-media-chat-drop');
  const icon = dragGhostAction.firstElementChild as HTMLElement | null;
  const label = dragGhostAction.lastElementChild as HTMLElement | null;
  if (icon) {
    render(createVNode(isChatDropTarget || isCollectionDropTarget || (showLabel && copy) ? IconAdd : IconPrev, {
      class: 'w-4 h-4',
    }), icon);
  }
  if (label) {
    label.textContent = showLabel
      ? (isChatDropTarget ? t('media_chat.add_to_turn') : isCollectionDropTarget
        ? t('collection.drop_action')
        : copy
        ? t('info_panel.drag_action.copy')
        : t('info_panel.drag_action.move'))
      : '';
    label.style.display = showLabel ? 'inline' : 'none';
  }
  dragGhostAction.style.padding = showLabel ? '0 9px 0 6px' : '0 5px';
  dragGhostAction.style.gap = showLabel ? '5px' : '0';
  dragGhostAction.style.background = showLabel && (copy || isCollectionDropTarget || isChatDropTarget)
    ? 'var(--color-success)'
    : 'color-mix(in oklab, var(--color-base-300) 94%, transparent)';
  dragGhostAction.style.color = showLabel && (copy || isCollectionDropTarget || isChatDropTarget)
    ? 'var(--color-success-content)'
    : 'var(--color-base-content)';
}

function updateDragGhostModifier(event: KeyboardEvent) {
  updateDragGhostAction(event);
}

function sanitizeDragGhostClone(element: HTMLElement) {
  const clone = element.cloneNode(true) as HTMLElement;
  clone.removeAttribute('id');
  clone.querySelectorAll('[id]').forEach(child => child.removeAttribute('id'));
  clone.querySelectorAll('button, input, [role="button"]').forEach(child => child.remove());
  clone.querySelectorAll('video').forEach(video => {
    video.muted = true;
    video.defaultMuted = true;
    video.volume = 0;
    video.pause();
    video.removeAttribute('autoplay');
  });
  clone.style.pointerEvents = 'none';
  clone.style.margin = '0';
  clone.style.width = `${element.getBoundingClientRect().width}px`;
  clone.style.height = `${element.getBoundingClientRect().height}px`;
  return clone;
}

function createDragGhost(
  draggedElement: HTMLElement,
  draggedFile: any,
  files: any[],
  fileCount = files.length,
  hotspot = { xRatio: 0.5, yRatio: 0.5 },
) {
  removeDragGhost();

  const firstFile = files[0] || draggedFile;
  const firstIndex = fileList.value.findIndex(file => Number(file?.id) === Number(firstFile?.id));
  const renderedFirst = firstIndex >= 0
    ? document.getElementById(`item-${firstIndex}`)
    : null;
  const sourceElement = renderedFirst || draggedElement;
  const front = sanitizeDragGhostClone(sourceElement);

  if (!renderedFirst && firstFile?.thumbnail) {
    const image = front.querySelector('img');
    if (image) image.src = firstFile.thumbnail;
  }

  const sourceRect = sourceElement.getBoundingClientRect();
  const scale = Math.min(1, 200 / Math.max(sourceRect.width, sourceRect.height, 1));
  const width = Math.max(1, Math.round(sourceRect.width * scale));
  const height = Math.max(1, Math.round(sourceRect.height * scale));
  dragGhostHotspotX = width * hotspot.xRatio;
  dragGhostHotspotY = height * hotspot.yRatio;
  const thumbnailElement = sourceElement.querySelector('[data-thumbnail-container]') as HTMLElement | null;
  const computedRadius = Number.parseFloat(
    getComputedStyle(thumbnailElement || sourceElement).borderTopLeftRadius,
  );
  const radius = Math.round((Number.isFinite(computedRadius) ? computedRadius : 8) * scale);
  const clipPath = `inset(0 round ${radius}px)`;
  front.style.width = `${sourceRect.width}px`;
  front.style.height = `${sourceRect.height}px`;
  front.style.transform = `scale(${scale})`;
  front.style.transformOrigin = 'top left';
  front.style.borderRadius = `${radius / scale}px`;
  front.style.overflow = 'hidden';
  front.querySelectorAll('img, video').forEach(media => {
    (media as HTMLElement).style.borderRadius = `${radius / scale}px`;
  });

  const ghost = document.createElement('div');
  ghost.style.position = 'fixed';
  ghost.style.left = '0';
  ghost.style.top = '0';
  ghost.style.width = `${width + (fileCount > 1 ? 12 : 0)}px`;
  ghost.style.height = `${height + (fileCount > 1 ? 12 : 0)}px`;
  ghost.style.pointerEvents = 'none';
  ghost.style.opacity = '0.5';
  ghost.style.zIndex = '2147483647';
  ghost.style.willChange = 'transform';

  if (fileCount > 1) {
    for (const offset of [12, 6]) {
      const layer = document.createElement('div');
      layer.style.position = 'absolute';
      layer.style.inset = `${offset}px 0 0 ${offset}px`;
      layer.style.width = `${width}px`;
      layer.style.height = `${height}px`;
      layer.style.borderRadius = `${radius}px`;
      layer.style.clipPath = clipPath;
      layer.style.background = 'color-mix(in oklab, var(--color-base-200) 92%, transparent)';
      layer.style.border = '1px solid color-mix(in oklab, var(--color-base-content) 25%, transparent)';
      layer.style.boxShadow = '0 4px 12px rgb(0 0 0 / 0.2)';
      ghost.appendChild(layer);
    }
  }

  const frontWrapper = document.createElement('div');
  frontWrapper.style.position = 'absolute';
  frontWrapper.style.left = '0';
  frontWrapper.style.top = '0';
  frontWrapper.style.width = `${width}px`;
  frontWrapper.style.height = `${height}px`;
  frontWrapper.style.overflow = 'hidden';
  frontWrapper.style.borderRadius = `${radius}px`;
  frontWrapper.style.clipPath = clipPath;
  frontWrapper.style.boxShadow = '0 8px 20px rgb(0 0 0 / 0.3)';
  frontWrapper.appendChild(front);
  ghost.appendChild(frontWrapper);

  if (fileCount > 1) {
    const badge = document.createElement('div');
    badge.textContent = fileCount.toLocaleString();
    badge.style.position = 'absolute';
    badge.style.right = '0';
    badge.style.bottom = '0';
    badge.style.minWidth = '24px';
    badge.style.height = '24px';
    badge.style.padding = '0 7px';
    badge.style.display = 'flex';
    badge.style.alignItems = 'center';
    badge.style.justifyContent = 'center';
    badge.style.borderRadius = '9999px';
    badge.style.background = 'var(--color-primary)';
    badge.style.color = 'var(--color-primary-content)';
    badge.style.border = '2px solid var(--color-base-100)';
    badge.style.fontSize = '12px';
    badge.style.fontWeight = '700';
    badge.style.lineHeight = '1';
    badge.style.boxShadow = '0 3px 10px rgb(0 0 0 / 0.35)';
    ghost.appendChild(badge);
  }

  const action = document.createElement('div');
  action.style.position = 'absolute';
  action.style.left = `${width / 2}px`;
  action.style.top = `${height / 2}px`;
  action.style.transform = 'translate(-50%, -50%)';
  action.style.height = '24px';
  action.style.padding = '0 9px 0 6px';
  action.style.display = 'inline-flex';
  action.style.alignItems = 'center';
  action.style.gap = '5px';
  action.style.borderRadius = '9999px';
  action.style.border = '1px solid color-mix(in oklab, var(--color-base-content) 18%, transparent)';
  action.style.fontSize = '12px';
  action.style.fontWeight = '650';
  action.style.lineHeight = '1';
  action.style.whiteSpace = 'nowrap';
  action.style.boxShadow = '0 3px 12px rgb(0 0 0 / 0.3)';
  action.style.backdropFilter = 'blur(8px)';

  const actionIcon = document.createElement('span');
  actionIcon.style.width = '14px';
  actionIcon.style.height = '14px';
  actionIcon.style.display = 'inline-flex';
  actionIcon.style.alignItems = 'center';
  actionIcon.style.justifyContent = 'center';
  actionIcon.style.fontSize = '15px';
  actionIcon.style.fontWeight = '800';

  const actionLabel = document.createElement('span');
  action.append(actionIcon, actionLabel);
  ghost.appendChild(action);

  document.body.appendChild(ghost);
  dragGhost = ghost;
  dragGhostAction = action;
}

function onContentWindowLeave(event: MouseEvent) {
  if (isContentInternalDrag.value && (event.buttons & 1)) void handOffNativeDrag();
}

async function handOffNativeDrag() {
  const exceedsLimit = pointerDragUsesSelection && selectedCount.value > MAX_NATIVE_DRAG_FILES;
  const filesPromise = nativeDragFiles;
  const preview = nativeDragPreview;
  let released = false;
  const onRelease = () => { released = true; };
  window.addEventListener('pointerup', onRelease, true);
  window.addEventListener('pointercancel', onRelease, true);
  gridViewRef.value?.cancelPointerDrag();
  // No PointerEvent means cleanup only: never execute an internal drop.
  await clearContentInternalDrag();
  try {
    if (exceedsLimit) {
      toast.warning(t('tooltip.drag_out.too_many', { count: MAX_NATIVE_DRAG_FILES }));
      return;
    }
    const files = await filesPromise;
    if (released || !files?.length) return;
    if (files.some(isOriginalUnavailable)) {
      toast.warning(t('offline.requires_original'));
      return;
    }
    await startNativeFileDrag(files.map(file => file.file_path), preview);
  } catch (error) {
    console.error('Native file drag failed:', error);
    toast.error(t('tooltip.drag_out.failed'));
  } finally {
    window.removeEventListener('pointerup', onRelease, true);
    window.removeEventListener('pointercancel', onRelease, true);
  }
}

function updateContentDragPosition(event: PointerEvent) {
  const elementAtPointer = document.elementFromPoint(event.clientX, event.clientY);
  const chatTarget = elementAtPointer?.closest('[data-media-chat-drop]');
  if (!chatTarget && isContentInternalDrag.value && (event.buttons & 1) && isWindowDragEdge(event)) {
    void handOffNativeDrag();
    return;
  }
  if (!dragGhost || (event.clientX === 0 && event.clientY === 0)) return;
  dragGhost.style.transform = `translate3d(${Math.round(event.clientX - dragGhostHotspotX)}px, ${Math.round(event.clientY - dragGhostHotspotY)}px, 0)`;
  if (elementAtPointer?.closest('[data-collection-tray-root]') && !config.collectionTray.expanded) {
    config.collectionTray.expanded = true;
    requestAnimationFrame(() => {
      if (dragGhost) {
        setPointerDropTarget(null);
        updateContentDragPosition(event);
      }
    });
  }
  const target = elementAtPointer
    ?.closest('[data-media-chat-drop], [data-file-drop-path][data-file-drop-album-id], [data-collection-drop-id], [data-collection-drop-new]') as HTMLElement | null;
  setPointerDropTarget(target);
  updateDragGhostAction(event);
}

function markContentInternalDrag({
  event,
  index,
  hotspotXRatio,
  hotspotYRatio,
}: {
  event: PointerEvent;
  index: number;
  hotspotXRatio: number;
  hotspotYRatio: number;
}) {
  if (boxSelection?.finishing && boxSelectionCurrent(boxSelection)) {
    gridViewRef.value?.cancelPointerDrag?.();
    return;
  }
  const draggedFile = fileList.value[index];
  const fileItem = document.getElementById(`item-${index}`);
  if (!fileItem || !isRealFileItem(draggedFile)) return;
  isContentInternalDrag.value = true;
  const selected = getActionableSelectedItems();
  pointerDragUsesSelection = Boolean(draggedFile.isSelected && selectedCount.value > 0);
  const files = pointerDragUsesSelection && selected.length > 0 ? selected : [draggedFile];
  draggedFileIds.value = new Set(files.map((file: any) => Number(file.id)).filter(id => id > 0));

  const canDragOut = !pointerDragUsesSelection || selectedCount.value <= MAX_NATIVE_DRAG_FILES;
  nativeDragPreview = canDragOut ? createDragPreview(fileItem.querySelector('img')) : [];
  nativeDragFiles = canDragOut
    ? (pointerDragUsesSelection ? getActionableSelectedItemsForAction() : Promise.resolve([draggedFile]))
    : null;
  pointerDragFiles = files.map((f: any) => ({
    id: f.id,
    file_path: f.file_path,
    folder_id: f.folder_id,
    album_id: f.album_id,
  }));
  createDragGhost(
    fileItem,
    draggedFile,
    files,
    pointerDragUsesSelection ? selectedCount.value : files.length,
    { xRatio: hotspotXRatio, yRatio: hotspotYRatio },
  );
  void tauriEmit('content-items-drag-state', { dragging: true });
  document.addEventListener('pointermove', updateContentDragPosition);
  document.documentElement.addEventListener('mouseleave', onContentWindowLeave);
  document.addEventListener('keydown', updateDragGhostModifier);
  document.addEventListener('keyup', updateDragGhostModifier);
  updateContentDragPosition(event);
}

async function clearContentInternalDrag(event?: PointerEvent) {
  const dragLibraryId = libConfig._libraryId;
  const target = pointerDropTarget;
  let files = pointerDragFiles;
  const usesSelection = pointerDragUsesSelection;
  const copy = event ? isCopyDragModifier(event) : false;
  const shouldDrop = event?.type !== 'pointercancel';
  isContentInternalDrag.value = false;
  draggedFileIds.value = new Set();
  pointerDragUsesSelection = false;
  pointerDragFiles = null;
  nativeDragFiles = null;
  nativeDragPreview = [];
  removeDragGhost();
  void tauriEmit('content-items-drag-state', { dragging: false });

  if (shouldDrop && event && target && files?.length) {
    if (usesSelection) {
      const selectedItems = await getActionableSelectedItemsForAction();
      if (!selectedItems) return;
      files = selectedItems.map((file: any) => ({
        id: file.id,
        file_path: file.file_path,
        folder_id: file.folder_id,
        album_id: file.album_id,
      }));
    }

    if (target.hasAttribute('data-media-chat-drop')) {
      if (dragLibraryId === libConfig._libraryId) target.dispatchEvent(new CustomEvent(MEDIA_CHAT_DROP_EVENT, { bubbles: true, detail: { libraryId: dragLibraryId, fileIds: files.map((file: any) => Number(file.id)).filter((id: number) => Number.isSafeInteger(id) && id > 0) } }));
      return;
    }
    const collectionId = target.dataset.collectionDropId;
    const collectionDropNew = target.dataset.collectionDropNew === 'true';
    if (collectionId || collectionDropNew) {
      let targetCollectionId = Number(collectionId || 0);
      if (collectionDropNew) {
        const collection = await createCollection(t('collection.default_name', { index: 1 }));
        targetCollectionId = Number(collection?.id || 0);
      }
      if (targetCollectionId <= 0) return;
      const result = await addFilesToCollection(targetCollectionId, files.map((file: any) => Number(file.id)).filter((id: number) => id > 0));
      if (result) {
        const added = Number(result.added || 0);
        const skipped = Number(result.skipped || 0);
        if (added > 0) {
          toast.success(t('collection.added_toast', { count: added }));
        }
        if (skipped > 0) {
          toast.info(t('collection.already_exists_toast', { count: skipped }));
        }
        const addedFileIds = new Set(files.map((file: any) => Number(file.id)).filter((id: number) => id > 0));
        for (const file of fileList.value) {
          if (addedFileIds.has(Number(file?.id))) {
            file.has_collections = true;
            file.collectionVersion = Number(file.collectionVersion || 0) + 1;
          }
        }
      }
      await tauriEmit('collection-files-dropped', {
        collectionId: targetCollectionId,
        count: Number(result?.added || 0),
        fileIds: files.map((file: any) => file.id),
      });
      if (libConfig.activePane === 'collection' && Number(libConfig.collection.selectedId || 0) === targetCollectionId) {
        await updateContent(true);
      }
      return;
    }

    // Collection drops only change the catalog; filesystem drops need every original.
    if (!await requireOriginalFiles(files)) return;
    if (!await confirmLargeBatch(files.length)) return;

    const destPath = String(target.dataset.fileDropPath || '');
    const albumId = Number(target.dataset.fileDropAlbumId || 0);
    const selected = destPath && albumId > 0 ? await selectFolder(albumId, destPath) : null;
    if (selected?.id) {
      let done = 0;
      let indexFailureCount = 0;
      let transferFailureCount = 0;
      const transferErrors: unknown[] = [];
      let applyAllPolicy: FileConflictPolicy | null = null;
      const affectedAlbumIds = new Set<number>([albumId]);
      const affectedFolderPaths = new Set<string>([destPath]);
      await withFileTransferBatch(async (batchId: number) => {
        for (const file of files) {
          if (!copy && file.folder_id === selected.id) continue;
          const conflict = await resolveConflictPolicy(
            file.file_path,
            getFolderName(file.file_path),
            destPath,
            applyAllPolicy,
            files.length > 1,
            !copy,
          );
          if (conflict.applyAll) applyAllPolicy = conflict.policy;
          if (conflict.policy === 'skip') continue;
          try {
            if (copy) {
              const copiedPath = await copyFile(file.file_path, destPath, conflict.policy, batchId);
              if (copiedPath) {
                if (await addFileToDb(selected.id, copiedPath)) done++;
                else indexFailureCount++;
              } else transferFailureCount++;
            } else {
              const movedPath = await moveFile(file.id, file.file_path, selected.id, destPath, conflict.policy, batchId);
              if (movedPath) {
                done++;
                affectedFolderPaths.add(getFolderPath(file.file_path));
                affectedAlbumIds.add(Number(file.album_id || 0));
              } else transferFailureCount++;
            }
          } catch (error) {
            transferFailureCount++;
            transferErrors.push(error);
          }
        }
      });
      if (done > 0) {
        await refreshFolderSidebarCounts([...affectedFolderPaths]);
        await refreshAffectedAlbums(Array.from(affectedAlbumIds));
        await refreshLibraryTotalCount();
        await tauriEmit('refresh-content');
      }
      if (indexFailureCount > 0) {
        toast.error(t('msgbox.copy_to_folder.index_error', {
          count: indexFailureCount.toLocaleString(),
        }));
      }
      if (transferFailureCount > 0) {
        toast.error(fileTransferFailure(t(copy ? 'msgbox.copy_to_folder.error' : 'msgbox.move_to_folder.error', {
          source: files.length === 1
            ? getFolderName(files[0].file_path)
            : t('toolbar.filter.select_count', { count: transferFailureCount.toLocaleString() }),
          dest: getFolderName(destPath) || destPath,
        }), transferErrors));
      }
    }
  }
}

const isProcessing = ref(false);  // show processing status
const isLoading = ref(false);     // show loading status in GridView (for empty file list)
const imageSearchError = ref(false);
const imageSearchLanguageUnsupported = ref(false);
const hasLoadedInitialResult = ref(false); // avoid showing "No files found" before first real result returns
const contentCountIsAuthoritative = ref(false);
const dedupSourceVersion = ref(0);

// Store current query params for virtual scrolling
const currentQueryParams = ref({
  searchFileType: 0,
  sortType: 0,
  sortOrder: 0,
  randomSeed: 0,
  searchFileName: "",
  searchAllSubfolders: "",
  searchFolder: "",
  startDate: 0,
  endDate: 0,
  calendarSort: 0,
  folderSort: 0,
  categorySort: 0,
  make: "",
  model: "",
  lensMake: "",
  lensModel: "",
  locationNodeId: 0,
  isFavorite: false,
  rating: -1,
  cullingFlag: -1,
  tagId: 0,
  tagGroupId: 0,
  personId: 0,
});
watch(() => [libConfig._libraryId, libConfig.album.id, currentQueryParams.value, effectiveGroupBy.value] as const,
  ([libraryId, albumId, params, groupBy]) => {
    if (config.main.sidebarIndex === SIDEBAR.ALBUM && libConfig.activePane !== 'collection') {
      setDlnaViewOrder(libraryId, Number(albumId), { ...params, groupBy });
    }
  }, { immediate: true });
const currentQuerySource = ref<'query' | 'smart' | 'collection' | 'search'>('query');
const objectBoxClassKey = computed(() =>
  tempViewMode.value === 'none' && currentQuerySource.value === 'query' &&
  config.main.sidebarIndex === SIDEBAR.OBJECTS && libConfig.activePane !== 'collection'
    ? String(libConfig.objects?.classKey || '') : '',
);
const isMapView = computed(() => config.settings.grid.viewMode === 'map');
const isGridLayoutView = computed(() => config.settings.grid.viewMode === 'grid');
const isFilmstripView = computed(() => config.settings.grid.viewMode === 'filmstrip');
const isMapVisible = computed(() => isMapView.value && !showQuickView.value && tempViewMode.value !== 'map');
const mapViewMounted = ref(isMapView.value);
const mapTempViewState = ref<{ lat: number; lon: number; zoom: number } | null>(null);
const currentSmartQueryParams = ref<any | null>(null);
const smartQueryError = ref('');
let smartRefreshTimer: ReturnType<typeof setTimeout> | undefined;
const smartEventStops: (()=>void)[] = [];
let smartEventsDisposed = false;
let smartClock:ReturnType<typeof setInterval>|undefined;
function refreshActiveSmartAlbum() {
  const album = activeSmartAlbum.value;
  if(!album || tempViewMode.value !== 'none') return;
  rememberFocusedFileForPresentationRefresh();captureSelectionForFileListRefresh();
  currentThumbRequestId++;
  void getSmartFileList(album, ++currentContentRequestId);
}
function queueSmartRefresh(){
  if(smartRefreshTimer)return;
  smartRefreshTimer=setTimeout(()=>{smartRefreshTimer=undefined;if(isLoading.value){queueSmartRefresh();return;}refreshActiveSmartAlbum();},800);
}
watch(()=>config.settings.ai?.semanticProfile,()=>{if(activeSmartAlbum.value && hasSemanticRules(activeSmartAlbum.value.query?.rules || []))queueSmartRefresh();});
watch(()=>config.settings.ai?.detectionProfile,()=>{if(activeSmartAlbum.value && hasObjectRules(activeSmartAlbum.value.query?.rules || []))queueSmartRefresh();});
watch(()=>config.settings.ai?.ocrProfile,()=>{if(activeSmartAlbum.value && hasOcrRules(activeSmartAlbum.value.query?.rules || []))queueSmartRefresh();});
watch(()=>uiStore.countUpdateTick,()=>{if(activeSmartAlbum.value)queueSmartRefresh();});
watch(()=>activeSmartAlbum.value?.name, name=>{if(name && tempViewMode.value==='none')contentTitle.value=name;});
onMounted(async()=>{
  smartClock=setInterval(()=>{if(activeSmartAlbum.value&&hasTimeSmartRules(activeSmartAlbum.value.query?.rules||[]))queueSmartRefresh();},60000);
  for(const name of ['object-index-updated','object-annotations-changed','ocr-index-updated','ocr-annotations-changed','media-lifecycle-changed','files-deleted','semantic-index-changed','smart-album-index-progress','index_progress','index_finished','face-person-changed','face_index_finished','location-resolution-progress','map-location-changed']) {
    const stop=await listen(name,(event:any)=>{const library=event.payload?.libraryId||event.payload?.library_id;if(library&&library!==libConfig._libraryId)return;if(name.startsWith('object-')&&(!activeSmartAlbum.value||!hasObjectRules(activeSmartAlbum.value.query?.rules||[])))return;if((name==='ocr-index-updated'||name==='ocr-annotations-changed')&&(!activeSmartAlbum.value||!hasOcrRules(activeSmartAlbum.value.query?.rules||[])))return;if(name==='face-person-changed'&&(isFaceRename(event.payload||{})||event.payload?.membershipChanged===false))return;queueSmartRefresh();});
    if(smartEventsDisposed){stop();return;}smartEventStops.push(stop);
  }
});
onBeforeUnmount(()=>{smartEventsDisposed=true;clearTimeout(smartRefreshTimer);clearInterval(smartClock);smartEventStops.forEach(stop=>stop());});
const currentCollectionId = ref<number | null>(null);
const currentSearchFileIds = ref<number[]>([]);
let currentMapSearchExactMembers = false;
const mapQueryParams = computed(() => (
  currentQuerySource.value === 'smart' && currentSmartQueryParams.value
    ? currentSmartQueryParams.value
    : currentQueryParams.value
));
const dedupSmartFileIds = ref<number[] | null>(null);
watch(isMapView, (active) => {
  if (active) mapViewMounted.value = true;
});

function passesAlbumFilters(file: any) {
  // Direct ID lookups also carry visibility computed from the owning album.
  return file?.album_visible !== false;
}

type SaveAsContext = {
  folderId?: number;
  collectionId?: number;
  tagId?: number;
};

// Capture this when opening the editor: the user can navigate elsewhere before saving.
// The editor window is reused, so contexts must remain tied to their source file.
const imageEditorSaveAsContexts = new Map<number, SaveAsContext | null>();

const getCurrentSaveAsContext = (file: any): SaveAsContext | null => {
  const folderId = Number(file?.folder_id || 0);
  const context: SaveAsContext = folderId > 0 ? { folderId } : {};
  const collectionId = Number(currentCollectionId.value || 0);
  if (currentQuerySource.value === 'collection' && collectionId > 0) {
    return { ...context, collectionId };
  }

  const tagId = Number(currentQueryParams.value.tagId || 0);
  if (currentQuerySource.value === 'query' && tagId > 0) {
    return { ...context, tagId };
  }

  return Object.keys(context).length > 0 ? context : null;
};

const scanStreamRequestInFlight = ref(false);
const scanStreamPullPending = ref(false);
const scanStreamAlbumId = ref<number | null>(null);
let scanStreamFlushTimer: ReturnType<typeof setTimeout> | null = null;
const scanStreamQueuedAlbumId = ref<number | null>(null);
const scanStreamQueuedIndexed = ref(0);
let scanVisiblePrefetchTimer: ReturnType<typeof setTimeout> | null = null;
const scanVisiblePrefetchStart = ref(0);
const scanVisiblePrefetchEnd = ref(0);
const pendingRestoreScrollTop = ref<number | null>(null);

const isFilmstripVertical = computed(() => isFilmstripView.value && config.settings.grid.previewPosition >= 2);

const libraryChecked = ref(false);

watch(() => props.libraryEmpty, () => {
  libraryChecked.value = true;
}, { immediate: true });

watch(contentReady, (ready) => {
  if (!ready) return;
  contentQueryRefreshPending = false;
  if (!contentCountIsAuthoritative.value || tempViewMode.value !== 'none') return;
  commitRequestedSidebarCount(totalFileCount.value);
});

function updateCount(owner: any, itemId: string | number, count: number) {
  owner.counts = {
    ...(owner.counts || {}),
    [String(itemId)]: Math.max(0, Number(count || 0)),
  };
}

async function refreshFolderSidebarCount(path: string) {
  const libraryId = libConfig._libraryId;
  const fileType = config.search.fileType;
  const showSubfolderFiles = config.settings.showSubfolderFiles;
  try {
    const result = await getQueryCountAndSum({
      searchFileName: '', searchFileType: fileType,
      sortType: 0, sortOrder: 0, searchAllSubfolders: showSubfolderFiles ? path : '',
      searchFolder: showSubfolderFiles ? '' : path,
      startDate: 0, endDate: 0, calendarSort: 0, make: '', model: '', lensMake: '', lensModel: '',
      locationNodeId: 0, isFavorite: false, rating: -1, cullingFlag: -1, tagId: 0, personId: 0,
    });
    if (result && libConfig._libraryId === libraryId
      && config.search.fileType === fileType && config.settings.showSubfolderFiles === showSubfolderFiles) {
      setFolderFileCount(path, Number(result[0] || 0));
    }
  } catch (error) {
    console.error('Failed to refresh folder count:', error);
  }
}

async function refreshFolderSidebarCounts(changedPaths: string[]) {
  const libraryId = libConfig._libraryId;
  const paths = new Map(changedPaths.filter(Boolean).map(path => [normalizePathForCompare(path), path]));
  if (config.settings.showSubfolderFiles) {
    for (const parent of getCountedFolderPaths()) {
      if (changedPaths.some(path => path && isWithinRootPath(path, parent))) {
        paths.set(normalizePathForCompare(parent), parent);
      }
    }
  }
  for (const path of paths.values()) {
    if (libConfig._libraryId !== libraryId) return;
    await refreshFolderSidebarCount(path);
  }
}

// Do not publish a previous query's total after switching folders.
const countedFolderRequestId = ref(-1);

watch(
  [totalFileCount, contentReady, isLoading, countedFolderRequestId],
  () => {
    const path = currentQueryParams.value.searchAllSubfolders || currentQueryParams.value.searchFolder;
    if (countedFolderRequestId.value !== currentContentRequestId || currentQuerySource.value !== 'query' || !path
      || !contentReady.value || isLoading.value || contentQueryRefreshPending
      || !contentCountIsAuthoritative.value || tempViewMode.value !== 'none'
      || !matchesRequestedSidebarCount({ source: 'album-folder', path })) return;
    setFolderFileCount(path, Math.max(0, Number(totalFileCount.value || 0)));
  },
);

function matchesRequestedSidebarCount(request: any) {
  switch (request?.source) {
    case 'library':
      return config.main.sidebarIndex === SIDEBAR.LIBRARY
        && libConfig.activePane === 'main'
        && libConfig.library.item === request.item
        && (request.item !== LIB_ITEM.RATINGS || Number(libConfig.rating.item) === Number(request.rating))
        && (request.item !== LIB_ITEM.CULLING || libConfig.culling.item === request.cullingItem)
        && (request.item !== LIB_ITEM.SUBJECTS || libConfig.library.smartId === request.smartId);
    case 'album':
      return config.main.sidebarIndex === SIDEBAR.ALBUM
        && libConfig.activePane === 'main'
        && libConfig.album.selected
        && Number(libConfig.album.id) === Number(request.id);
    case 'album-folder':
      return config.main.sidebarIndex === SIDEBAR.ALBUM
        && libConfig.activePane === 'main'
        && !libConfig.album.selected
        && libConfig.album.folderPath === request.path;
    case 'collection':
      return libConfig.activePane === 'collection'
        && Number(libConfig.collection.selectedId) === Number(request.id);
    case 'tag':
      return config.main.sidebarIndex === SIDEBAR.TAG && Number(libConfig.tag.id) === Number(request.id);
    case 'smart-album':
      return config.main.sidebarIndex === SIDEBAR.SMART_ALBUM
        && libConfig.smartAlbum.type === 'custom'
        && String(libConfig.smartAlbum.id) === String(request.id);
    case 'search':
      return config.main.sidebarIndex === SIDEBAR.SEARCH && !isStructuredSearchView.value && libConfig.search.searchText === request.text;
    default:
      return false;
  }
}

function commitRequestedSidebarCount(count: number, request = uiStore.countUpdateRequest) {
  if (!request || uiStore.countUpdateRequest !== request) return;
  if (!matchesRequestedSidebarCount(request)) {
    uiStore.clearCountUpdateRequest();
    return;
  }

  switch (request.source) {
    case 'library': {
      const library = libConfig.library as any;
      if (request.item === LIB_ITEM.SUBJECTS) {
        library.subjectCounts = {
          ...(library.subjectCounts || {}),
          [String(request.smartId)]: Math.max(0, Number(count || 0)),
        };
      } else {
        const cachedValues = library.counts || {};
        const defaults = createEmptyLibraryCounts();
        const values = {
          ...defaults,
          ...cachedValues,
          ratings: { ...defaults.ratings, ...(cachedValues.ratings || {}) },
          culling: { ...defaults.culling, ...(cachedValues.culling || {}) },
        };
        if (request.item === LIB_ITEM.ALL) values.all = count;
        else if (request.item === LIB_ITEM.FAV) values.favorite = count;
        else if (request.item === LIB_ITEM.TODAY) values.today = count;
        else if (request.item === LIB_ITEM.RATINGS) {
          if (Number(request.rating) === RATE.ALL) values.rated = count;
          else if (Number(request.rating) === RATE.UNRATED) values.unrated = count;
          else values.ratings = { ...(values.ratings || {}), [Number(request.rating)]: count };
        } else if (request.item === LIB_ITEM.CULLING) {
          values.culling = { ...(values.culling || {}), [String(request.cullingItem)]: count };
        }
        library.counts = values;
      }
      break;
    }
    case 'album':
      updateCount(libConfig.album, request.id, count);
      break;
    case 'album-folder':
      setFolderFileCount(request.path, Math.max(0, Number(count || 0)));
      break;
    case 'collection':
      updateCount(libConfig.collection, request.id, count);
      break;
    case 'tag':
      updateCount(libConfig.tag, request.id, count);
      break;
    case 'smart-album': {
      const albums = [...(libConfig.smartAlbums || [])];
      const index = albums.findIndex((album: any) => String(album.id) === String(request.id));
      if (index >= 0) {
        albums[index] = {
          ...albums[index],
          count: Math.max(0, Number(count || 0)),
        };
        libConfig.smartAlbums = albums;
      }
      break;
    }
    case 'search': {
      const history = [...(libConfig.search.searchHistory || [])] as any[];
      const index = history.findIndex((item: any) => (typeof item === 'string' ? item : item?.text) === request.text);
      if (index >= 0) {
        const item = history[index];
        history[index] = {
          ...(typeof item === 'string' ? { text: item } : item),
          count: Math.max(0, Number(count || 0)),
        };
        libConfig.search.searchHistory = history;
      }
      break;
    }
  }
  uiStore.clearCountUpdateRequest();
}

const showWelcomeContent = computed(() => props.libraryEmpty && libraryChecked.value);
const hasConfirmedEmptyContent = computed(() => (
  contentReady.value
  && !isLoading.value
  && totalFileCount.value === 0
));
const showFilmstripLayout = computed(() => (
  isFilmstripView.value
  && !showWelcomeContent.value
  && !hasConfirmedEmptyContent.value
));
const isMapContentVisible = computed(() => isMapVisible.value && !showWelcomeContent.value);

const gridViewLayoutClass = computed(() => {
  const pos = config.settings.grid.previewPosition || 0;
  if (pos === 0) return 'flex-col';
  if (pos === 1) return 'flex-col-reverse';
  if (pos === 2) return 'flex-row';
  if (pos === 3) return 'flex-row-reverse';
  return 'flex-col';
});

// ai image search params
const currentImageSearchParams = ref({
  searchText: "",
  fileId: 0,
  threshold: 0,
});

function showEmptyContent(requestId: number) {
  if (requestId !== currentContentRequestId) return;
  clearSelectionForFileListUpdate();
  clearContentRows();
  totalFileCount.value = 0;
  totalFileSize.value = 0;
  contentCountIsAuthoritative.value = true;
  timelineData.value = [];
  lastVisibleRange = { start: -1, end: -1 };
  visibleRangeSeqId++;
  markDedupSourceUpdated(requestId);
  openImageViewer(0, false, true);
  isLoading.value = false;
  hasLoadedInitialResult.value = true;
  contentReady.value = true;
}

function showLoadingContent(requestId: number) {
  if (requestId !== currentContentRequestId) return;
  clearSelectionForFileListUpdate();
  clearContentRows();
  totalFileCount.value = 0;
  totalFileSize.value = 0;
  contentCountIsAuthoritative.value = false;
  timelineData.value = [];
  isLoading.value = true;
  contentReady.value = false;
}

// Similar Search Mode State
const tempViewMode = ref<'none' | 'similar' | 'album' | 'person' | 'camera' | 'lens' | 'location' | 'map'>('none');
let suppressPersonContextRefresh = false;
const dedupQueryParams = computed(() => {
  return { ...currentQueryParams.value };
});
const dedupCollectionId = computed(() =>
  currentQuerySource.value === 'collection' ? currentCollectionId.value : null
);
const dedupFileIds = computed(() =>
  currentQuerySource.value === 'search'
    ? [...currentSearchFileIds.value]
    : currentQuerySource.value === 'smart'
      ? dedupSmartFileIds.value
      : null
);

const dedupScanKey = computed(() => {
  if (dedupSourceVersion.value <= 0) return '';
  if (currentQuerySource.value === 'smart' && dedupFileIds.value === null) return '';
  return `query:${JSON.stringify(dedupQueryParams.value)}|collection:${dedupCollectionId.value ?? ''}|files:${JSON.stringify(dedupFileIds.value)}`;
});
const similarPhotoGroupingThreshold = computed(() => {
  const index = config.settings.similarPhotos?.groupingThresholdIndex ?? 1;
  return config.similarPhotoGroupingThresholds[index] ?? 0.93;
});
const similarViewVersion = ref(0);
const similarScanKey = ref('');
watch(() => JSON.stringify([config.settings.ai?.semanticModel, config.settings.ai?.semanticProfile, config.settings.ai?.semanticParameters, config.settings.ai?.semanticLanguages]), () => {
  if (currentQuerySource.value === 'search') void updateContent(true);
}, { deep: true });
watch([dedupScanKey, similarPhotoGroupingThreshold, () => config.settings.ai?.semanticProfile], ([key, threshold]) => {
  similarScanKey.value = key
    ? `${key}|threshold:${threshold}|similar-view:${++similarViewVersion.value}`
    : '';
}, { immediate: true });

const currentTitleIcon = computed(() => {
  if (libConfig.activePane === 'collection') return IconStack;
  switch (tempViewMode.value) {
    case 'none':
      if (contentTitle.value) {
        switch (config.main.sidebarIndex) {
          case SIDEBAR.LIBRARY:
            switch (libConfig.library.item) {
              case LIB_ITEM.ALL: return IconFiles;
              case LIB_ITEM.MEDIA:
              case LIB_ITEM.HIDDEN:
              case LIB_ITEM.DELETED: return IconFiles;
      case LIB_ITEM.FAV: return IconHeartFilled;
              case LIB_ITEM.RATINGS: return libConfig.rating.item > 0 || libConfig.rating.item === RATE.ALL ? IconStarFilled : IconStar;
              case LIB_ITEM.CULLING:
                return libConfig.culling.item === CULLING.PICK
                  ? IconFlagFilled
                  : libConfig.culling.item === CULLING.REJECT
                    ? IconFlagOff
                    : IconFlag;
              case LIB_ITEM.SUBJECTS: return IconBolt;
              case LIB_ITEM.TODAY: return IconHistory;
              default: return IconFiles;
            }
          case SIDEBAR.ALBUM:
              return libConfig.album.selected || config.settings.showSubfolderFiles ? IconFolders : IconFolder;
          case SIDEBAR.SMART_ALBUM: return IconFolderCog;
          case SIDEBAR.SEARCH: return IconSearch;
          case SIDEBAR.CALENDAR:
            return config.calendar.view === 'months'
              ? IconCalendarMonth
              : config.calendar.view === 'days'
                ? IconCalendarDay
                : IconCalendar;
          case SIDEBAR.TAG: return IconTag;
          case SIDEBAR.OCR: return IconSearch;
          case SIDEBAR.OBJECTS: return IconBox;
          case SIDEBAR.PERSON: return IconPerson;
          case SIDEBAR.LOCATION: return IconLocation;
          case SIDEBAR.CAMERA: return config.camera.isCamera ? IconCamera : IconCameraAperture;
          default: return IconFiles;
        }
      }
      return null;
    case 'similar': return IconPhotoSearch;
    case 'album': return IconFolderSearch;
    case 'person': return IconPersonSearch;
    case 'camera': return IconCamera;
    case 'lens': return IconCameraAperture;
    case 'location': return IconLocation;
    default: return null;
  }
});

const backupState = ref<any>(null);

let unlistenAlbumAccessChanged: (() => void) | undefined;
let unlistenKeydown: () => void;
let unlistenImageViewer: () => void;
let unlistenImageEditor: (() => void) | null = null;
let unlistenMontage: (() => void) | null = null;
let unlistenFaceIndexProgress: (() => void) | null = null;
let unlistenLibraryTotalRefreshed: (() => void) | null = null;
let unlistenImportFilesAdded: (() => void) | null = null;
let unlistenImportFilesToFolder: (() => void) | null = null;
let unlistenPasteClipboard: (() => void) | null = null;

let resizeObserver: ResizeObserver | null = null;
let contentUpdateTimer: ReturnType<typeof setTimeout> | null = null;
let contentQueryRefreshPending = false;
let sidebarCountRequestId = 0;

function scheduleContentRefresh(task: () => void) {
  if (contentUpdateTimer) {
    clearTimeout(contentUpdateTimer);
  }
  contentUpdateTimer = setTimeout(() => {
    contentUpdateTimer = null;
    task();
  }, 0);
}

function waitForContentLoadingPaint() {
  return new Promise<void>(resolve => {
    requestAnimationFrame(() => setTimeout(resolve, 0));
  });
}

function refreshContentFromSelectionChange() {
  // In single-select mode, retain the active file before the config watcher
  // resets the viewport. This covers grouping changes as well as sorting and
  // filters that are not initiated by a toolbar handler.
  if (!selectMode.value && selectedItemIndex.value >= 0) {
    rememberFocusedFileForPresentationRefresh();
  }

  if (!selectMode.value || selectedCount.value === 0) {
    scrollPosition.value = 0;
    gridViewRef.value?.scrollToPosition(0);
    // Keep the file index intact until its new position is resolved. A view
    // change clears pendingFocusedFileId and therefore retains the old reset.
    if (!pendingFocusedFileId) selectedItemIndex.value = 0;
  }
  updateContent();
  // Reset ImageViewer context if open (without focusing/showing it)
  openImageViewer(selectedItemIndex.value, false, true);
}

onMounted(() => {
  if (gridScrollContainerRef.value) {
    resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        containerHeight.value = entry.contentRect.height;
        containerWidth.value = entry.contentRect.width;
      }
    });
    resizeObserver.observe(gridScrollContainerRef.value);
    
    // Initial values
    containerHeight.value = gridScrollContainerRef.value.clientHeight;
    containerWidth.value = gridScrollContainerRef.value.clientWidth;
  }

  migrateRightPanelWidthToPixels();
  window.addEventListener('resize', handleWindowResize);
});

onBeforeUnmount(() => {
  fileImportDisposed = true;
  selectionAccessRequest++;
  if (selectionAccessTimer) clearTimeout(selectionAccessTimer);
  stopSlideShow();
  clearRightPanelAnimationTimer();
  if (contentUpdateTimer) {
    clearTimeout(contentUpdateTimer);
    contentUpdateTimer = null;
  }
  window.removeEventListener('resize', handleWindowResize);
  if (resizeObserver) {
    resizeObserver.disconnect();
  }
  if (unlistenKeydown) unlistenKeydown();
  if (unlistenImageViewer) unlistenImageViewer();
  if (unlistenImageEditor) unlistenImageEditor();
  if (unlistenLibraryTotalRefreshed) unlistenLibraryTotalRefreshed();
  if (unlistenImportFilesAdded) unlistenImportFilesAdded();
});

async function refreshImportedAlbumContent(albumId: number) {
  if (
    config.main.sidebarIndex !== SIDEBAR.ALBUM
    || Number(libConfig.album.id || 0) !== albumId
  ) return;

  const requestId = ++currentContentRequestId;
  if (libConfig.album.selected) {
    await getFileList({ searchAllSubfolders: libConfig.album.folderPath }, requestId);
    return;
  }

  const folderPath = libConfig.album.folderPath || '';
  if (!folderPath) return;
  await getFileList(
    config.settings.showSubfolderFiles
      ? { searchAllSubfolders: folderPath }
      : { searchFolder: folderPath },
    requestId,
  );
}

// New event handlers for GridView
function handleItemClicked(
  index: number,
  modifiers: { shiftKey?: boolean; metaKey?: boolean; ctrlKey?: boolean } = {}
) {
  boxSelection = null;
  if (!ensureGroupedFileAtIndex(index)) return;
  const shiftKey = !!modifiers.shiftKey;
  const toggleSelection = !!(modifiers.metaKey || modifiers.ctrlKey);

  if (!selectMode.value && shiftKey && selectedItemIndex.value >= 0 && selectedItemIndex.value !== index) {
    checkUnsavedChanges(() => {
      void selectRangeFromSingleSelection(selectedItemIndex.value, index);
    });
    return;
  }

  if (!selectMode.value && toggleSelection && selectedItemIndex.value >= 0) {
    checkUnsavedChanges(() => {
      const anchorIndex = selectedItemIndex.value;
      handleSelectMode(true);
      setItemSelected(anchorIndex, true);
      if (isGridLayoutView.value && config.rightPanel.show) selectionPanelOpen.value = true;

      selectedItemIndex.value = index;
      if (index !== anchorIndex) {
        void handleItemSelectToggled(index);
      }
    });
    return;
  }

  if (index === selectedItemIndex.value) {
    if (selectMode.value) {
      void handleItemSelectToggled(index, shiftKey);
    }
    return;
  }
  
  checkUnsavedChanges(() => {
    selectedItemIndex.value = index;
    if (selectMode.value) {
      void handleItemSelectToggled(index, shiftKey);
    } else {
      lastSelectedIndex.value = index;
    }
  });
}

// Double click grid view item
function handleItemDblClicked(
  index: number,
  modifiers: { shiftKey?: boolean; metaKey?: boolean; ctrlKey?: boolean } = {}
) {
  if (!ensureGroupedFileAtIndex(index)) return;
  const openInNewWindow = !!(
    modifiers.shiftKey || modifiers.metaKey || modifiers.ctrlKey
  );
  if (openInNewWindow) {
    checkUnsavedChanges(() => {
      selectedItemIndex.value = index;
      openImageViewer(index, true);
    });
    return;
  }

  if (index === selectedItemIndex.value) {
    if (!isFilmstripView.value) {
      // quick view
      showQuickView.value = true;
      quickViewZoomFit.value = true;
    }
    return;
  }
  
  checkUnsavedChanges(() => {
    selectedItemIndex.value = index;

    if (!isFilmstripView.value) {
      // quick view
      showQuickView.value = true;
      quickViewZoomFit.value = true;
    }
  });
}

// Track last selected index for shift-click range selection
const lastSelectedIndex = ref(-1);
const keyboardSelectionAnchorIndex = ref(-1);

type BoxSelection = {
  id: number; libraryId: string; queryId: number; files: any[]; ready: boolean; additive: boolean; indices: number[];
  original: { ids: Set<number>; count: number; size: number; groups: Record<string, number>; groupSizes: Record<string, number>; mode: boolean; panel: boolean };
  baseline: Set<number>; hits: Map<number, number>; finishing?: boolean; pendingEnd?: { id: number; cancelled: boolean; rows: number[] };
};
let boxSelection: BoxSelection | null = null;
function boxSelectionCurrent(state: BoxSelection) {
  return boxSelection === state
    && state.libraryId === libConfig._libraryId
    && state.queryId === currentContentRequestId
    && state.files === fileList.value;
}
function beginBoxSelection(payload: { id: number; additive: boolean }) {
  if (isScanStreamingMode.value || !isGridLayoutView.value) return;
  const state: BoxSelection = {
    ...payload,
    libraryId: libConfig._libraryId,
    queryId: currentContentRequestId,
    files: fileList.value,
    ready: false,
    indices: [],
    original: {
      ids: new Set(selectedFileIds),
      count: selectedCount.value,
      size: selectedSize.value,
      groups: { ...groupSelectedCountMap.value },
      groupSizes: { ...groupSelectedSizeMap.value },
      mode: selectMode.value,
      panel: selectionPanelOpen.value,
    },
    baseline: new Set(),
    hits: new Map(),
  };
  boxSelection = state;
  checkUnsavedChanges(() => {
    if (!boxSelectionCurrent(state)) return;
    handleSelectMode(true);
    if (state.additive && !state.original.mode && isRealFileItem(fileList.value[selectedItemIndex.value])) setItemSelected(selectedItemIndex.value, true);
    state.baseline = new Set(selectedFileIds);
    if (!state.additive) {
      clearLoadedSelectionFlags();
      resetSelectionSummary();
      groupSelectedCountMap.value = {};
      groupSelectedSizeMap.value = {};
    }
    state.ready = true;
    updateBoxSelection({ id: state.id, indices: state.indices });
    if (state.pendingEnd) void finishBoxSelection(state.pendingEnd);
  });
}
function updateBoxSelection(payload: { id: number; indices: number[] }) {
  const state = boxSelection;
  if (!state || state.id !== payload.id || !boxSelectionCurrent(state)) return;
  state.indices = payload.indices;
  if (!state.ready) return;
  const hits = new Map<number, number>();
  for (const index of payload.indices) {
    if (!Number.isSafeInteger(index) || index < 0) continue;
    const file = fileList.value[index];
    if (isRealFileItem(file) && Number(file.id) > 0) hits.set(Number(file.id), index);
  }
  for (const [fileId, index] of state.hits) {
    if (!hits.has(fileId)) setItemSelected(index, state.additive && state.baseline.has(fileId));
  }
  for (const [fileId, index] of hits) {
    if (!selectedFileIds.has(fileId)) setItemSelected(index, true);
  }
  state.hits = hits;
}
async function finishBoxSelection(payload: { id: number; cancelled: boolean; rows: number[] }) {
  const state = boxSelection;
  if (!state || state.id !== payload.id) return;
  if (!boxSelectionCurrent(state)) { boxSelection = null; return; }
  if (payload.cancelled) {
    boxSelection = null;
    if (state.ready) {
      selectedFileIds.clear();
      for (const id of state.original.ids) selectedFileIds.add(id);
      for (const file of fileList.value) if (file) file.isSelected = state.original.ids.has(Number(file.id));
      selectedCount.value = state.original.count;
      selectedSize.value = state.original.size;
      groupSelectedCountMap.value = state.original.groups;
      groupSelectedSizeMap.value = state.original.groupSizes;
      selectMode.value = state.original.mode;
      selectionPanelOpen.value = state.original.panel;
      syncSelectionVersions();
    }
    return;
  }
  if (!state.ready) { state.pendingEnd = payload; return; }
  state.finishing = true;
  const rows = [...new Set(payload.rows.filter(index => Number.isSafeInteger(index) && index >= 0))].sort((a, b) => a - b);
  let groupedProcessing = false;
  try {
    if (rows.length && groupedModeActive.value) {
      const chunkSize = selectionChunkSize.value;
      const chunks = [...new Set(rows.filter(index => {
        const row = groupedRows.value[index];
        return !row || row.isPlaceholder || !isGroupRow(row) && !isRealFileItem(row.file);
      }).map(index => Math.floor(index / chunkSize) * chunkSize))];
      if (chunks.length) { isProcessing.value = true; groupedProcessing = true; }
      for (const start of chunks) {
        const result = await getCurrentGroupedQueryRows(start, Math.min(chunkSize, totalRowCount.value - start));
        if (!boxSelectionCurrent(state)) return;
        const normalized = normalizeGroupedRowsResult(result);
        applyGroupedRows(normalized.rows, start);
        scheduleLayoutRefresh();
      }
      const unresolved = rows.some(index => {
        const row = groupedRows.value[index];
        return !row || row.isPlaceholder || !isGroupRow(row) && !isRealFileItem(fileList.value[Number(row.file_index)]);
      });
      if (unresolved) { toast.error(t('info_panel.selection_load_failed')); return; }
    } else if (rows.length && !await hydrateSelectionRange(rows[0], rows[rows.length - 1])) {
      if (boxSelectionCurrent(state)) toast.error(t('info_panel.selection_load_failed'));
    }
    if (!boxSelectionCurrent(state)) return;
    const indices = rows.flatMap(index => {
      if (!groupedModeActive.value) return [index];
      const row = groupedRows.value[index];
      return isGroupRow(row) ? [] : [Number(row?.file_index)];
    });
    updateBoxSelection({ id: state.id, indices });
  } catch (error) { if (boxSelectionCurrent(state)) { console.error('finishBoxSelection error:', error); toast.error(t('info_panel.selection_load_failed')); } }
  finally {
    if (groupedProcessing && state.libraryId === libConfig._libraryId && state.queryId === currentContentRequestId && state.files === fileList.value) isProcessing.value = false;
    if (boxSelection === state) boxSelection = null;
  }
}

async function hydrateSelectionRange(startIndex: number, endIndex: number) {
  const start = Math.max(0, Math.min(startIndex, endIndex));
  const end = Math.min(fileList.value.length - 1, Math.max(startIndex, endIndex));
  if (start > end) return false;

  if (groupedModeActive.value) {
    const rowStart = getGroupedRowIndexForFileIndex(start);
    const rowEnd = getGroupedRowIndexForFileIndex(end);
    if (rowStart < 0 || rowEnd < 0) return false;
    await fetchGroupedRowsRange(Math.min(rowStart, rowEnd), Math.max(rowStart, rowEnd) + 1);
    for (let i = start; i <= end; i++) {
      if (!isRealFileItem(fileList.value[i]) && !ensureGroupedFileAtIndex(i)) {
        return false;
      }
    }
    return true;
  }

  const hasPlaceholders = fileList.value
    .slice(start, end + 1)
    .some(isPendingFileItem);
  return !hasPlaceholders || await hydrateRangeForSelection(start, end + 1);
}

async function selectRangeFromSingleSelection(anchorIndex: number, targetIndex: number) {
  const start = Math.min(anchorIndex, targetIndex);
  const end = Math.max(anchorIndex, targetIndex);
  if (!await hydrateSelectionRange(start, end)) {
    toast.error(t('info_panel.selection_load_failed'));
    return;
  }

  clearLoadedSelectionFlags();
  resetSelectionSummary();
  groupSelectedCountMap.value = {};
  groupSelectedSizeMap.value = {};
  selectMode.value = true;
  showQuickView.value = false;
  stopSlideShow();

  for (let i = start; i <= end; i++) {
    if (isRealFileItem(fileList.value[i])) {
      setItemSelected(i, true);
    }
  }

  selectedItemIndex.value = targetIndex;
  lastSelectedIndex.value = targetIndex;
  if (isGridLayoutView.value && config.rightPanel.show) selectionPanelOpen.value = true;
}

async function handleItemSelectToggled(index: number, shiftKey: boolean = false) {
  boxSelection = null;
  if (!ensureGroupedFileAtIndex(index)) return;
  const targetItem = fileList.value[index];
  if (!targetItem) return;

  if (shiftKey && lastSelectedIndex.value !== -1 && lastSelectedIndex.value !== index) {
    // Range selection: select all items between lastSelectedIndex and index
    const start = Math.min(lastSelectedIndex.value, index);
    const end = Math.max(lastSelectedIndex.value, index);
    
    // Set all items in range to the same selection state as the target item
    const targetState = !targetItem.isSelected;
    if (!await hydrateSelectionRange(start, end)) {
      toast.error(t('info_panel.selection_load_failed'));
      return;
    }
    for (let i = start; i <= end; i++) {
      if (isRealFileItem(fileList.value[i])) {
        setItemSelected(i, targetState);
      }
    }
  } else {
    // Single toggle
    setItemSelected(index, !targetItem.isSelected);
  }

  // Checkbox clicks do not bubble to handleItemClicked. Keep the operated
  // thumbnail as the current item even when this click unchecks it.
  selectedItemIndex.value = index;
  
  // Update last selected index
  lastSelectedIndex.value = index;
  if (isGridLayoutView.value && config.rightPanel.show) selectionPanelOpen.value = true;
}

function toggleKeyboardSelection(direction: 'prev' | 'next') {
  boxSelection = null;
  if (getActivePreviewMode() !== 'none') return false;

  const currentIndex = selectedItemIndex.value;
  const nextIndex = direction === 'next' ? currentIndex + 1 : currentIndex - 1;
  if (currentIndex < 0 || nextIndex < 0 || nextIndex >= fileList.value.length) return false;
  if (!ensureGroupedFileAtIndex(nextIndex)) return false;

  checkUnsavedChanges(() => {
    void (async () => {
      if (keyboardSelectionAnchorIndex.value < 0) {
        keyboardSelectionAnchorIndex.value = currentIndex;
      }

      if (!selectMode.value) {
        handleSelectMode(true);
      }

      const start = Math.min(keyboardSelectionAnchorIndex.value, nextIndex);
      const end = Math.max(keyboardSelectionAnchorIndex.value, nextIndex);
      if (!await hydrateSelectionRange(start, end)) {
        toast.error(t('info_panel.selection_load_failed'));
        return;
      }
      for (let i = start; i <= end; i++) {
        if (isRealFileItem(fileList.value[i])) {
          setItemSelected(i, true);
        }
      }

      selectedItemIndex.value = nextIndex;
      lastSelectedIndex.value = nextIndex;
    })();
  });
  return true;
}

async function handleDateGroupSelect({ startIndex, endIndex, selected }: { startIndex: number; endIndex: number; selected: boolean }) {
  boxSelection = null;
  if (!selectMode.value) return;
  const start = Math.max(0, Math.min(Number(startIndex || 0), fileList.value.length));
  const end = Math.max(start, Math.min(Number(endIndex || start), fileList.value.length));
  if (start >= end) return;

  const needsLoad = fileList.value.slice(start, end).some(isPendingFileItem);
  if (needsLoad) {
    await fetchDataRange(start, end);
  }

  for (let i = start; i < end; i++) {
    if (isRealFileItem(fileList.value[i])) {
      setItemSelected(i, selected);
    }
  }

  lastSelectedIndex.value = selected ? start : -1;
}

async function handleGroupSelectToggled(groupRow: any, selected: boolean) {
  boxSelection = null;
  const groupId = String(groupRow?.group_id || '');
  if (!groupId || groupSelectionLoading.value[groupId]) return;

  groupSelectionLoading.value = {
    ...groupSelectionLoading.value,
    [groupId]: true,
  };

  try {
    const ids = await getCachedGroupFileIds(groupId);
    if (!ids || ids.length === 0) return;

    selectMode.value = true;
    const idSet = new Set(ids.map(id => Number(id)).filter(id => Number.isFinite(id) && id > 0));
    const loadedById = new Map<number, number>();
    fileList.value.forEach((file, index) => {
      const fileId = Number(file?.id || 0);
      if (fileId) loadedById.set(fileId, index);
    });

    for (const fileId of idSet) {
      if (selected) selectedFileIds.add(fileId);
      else selectedFileIds.delete(fileId);

      const index = loadedById.get(fileId);
      if (index === undefined) continue;
      const file = fileList.value[index];
      if (isRealFileItem(file)) {
        file.isSelected = selected;
      }
    }

    const currentGroupCount = Number(groupSelectedCountMap.value[groupId] || 0);
    const currentGroupSize = Number(groupSelectedSizeMap.value[groupId] || 0);
    const nextGroupCount = selected ? Number(groupRow.count || idSet.size) : 0;
    const nextGroupSize = selected ? Number(groupRow.size || groupMetaMap.get(groupId)?.size || 0) : 0;
    updateGroupSelectedCount(groupId, nextGroupCount);
    updateGroupSelectedSize(groupId, nextGroupSize);
    selectedCount.value = Math.max(0, selectedCount.value + (nextGroupCount - currentGroupCount));
    selectedSize.value = Math.max(0, selectedSize.value + (nextGroupSize - currentGroupSize));

    if (selected) {
      let firstFileIndex = 0;
      for (const group of groupedTimelineGroups.value) {
        if (String(group.groupId) === groupId) {
          selectedItemIndex.value = firstFileIndex;
          lastSelectedIndex.value = firstFileIndex;
          break;
        }
        firstFileIndex += Number(group.count || 0);
      }
    }

    syncSelectionVersions();
  } catch (error) {
    console.error('handleGroupSelectToggled error:', error);
    toast.error(t('info_panel.selection_load_failed'));
  } finally {
    groupSelectionLoading.value = {
      ...groupSelectionLoading.value,
      [groupId]: false,
    };
  }
}

async function getCachedGroupFileIds(groupId: string) {
  const cached = groupFileIdsCache.get(groupId);
  if (cached) return cached;

  const result = currentQuerySource.value === 'collection' && currentCollectionId.value
    ? await getCollectionGroupFileIds(currentCollectionId.value, getGroupingQueryParams(), groupId)
    : currentQuerySource.value === 'smart' && currentSmartQueryParams.value
      ? await getSmartGroupFileIds(getGroupingQueryParams(), groupId)
      : await getGroupFileIds(getGroupingQueryParams(), groupId);
  const ids = Array.isArray(result)
    ? result
    : Array.isArray(result?.file_ids)
      ? result.file_ids
      : Array.isArray(result?.fileIds)
        ? result.fileIds
        : [];
  const normalized = ids.map((id: any) => Number(id)).filter((id: number) => Number.isFinite(id) && id > 0);
  groupFileIdsCache.set(groupId, normalized);
  for (const id of normalized) fileIdToGroupId.set(id, groupId);
  return normalized;
}

function unselectFileFromSelection(fileId: number) {
  boxSelection = null;
  const targetId = Number(fileId);
  const file = fileList.value.find(item => Number(item?.id || 0) === targetId);
  if (!file) return;
  const index = fileList.value.indexOf(file);
  setItemSelected(index, false);
}

async function handleTimelineSelectItem(index: number) {
  if (groupedModeActive.value) {
    if (index < 0 || index >= totalRowCount.value) return;
    const fileIndex = getGroupedFileIndexForRowIndex(index);
    if (fileIndex < 0 || fileIndex >= fileList.value.length) return;
    const rowIndex = getGroupedRowIndexForFileIndex(fileIndex);
    if (rowIndex >= 0) {
      await fetchGroupedRowsRange(rowIndex, rowIndex + 1);
    }
    if (!ensureGroupedFileAtIndex(fileIndex)) return;
    if (fileIndex === selectedItemIndex.value) return;

    checkUnsavedChanges(() => {
      selectedItemIndex.value = fileIndex;
      if (!selectMode.value) {
        lastSelectedIndex.value = fileIndex;
      }
    });
    return;
  }

  if (index < 0 || index >= fileList.value.length) return;
  if (index === selectedItemIndex.value) return;

  checkUnsavedChanges(() => {
    selectedItemIndex.value = index;
  });
}

function currentOriginalsUnavailable() {
  if (selectMode.value && selectionHasUnavailable.value) return true;
  const files = selectMode.value ? getActionableSelectedItems() : [fileList.value[selectedItemIndex.value]];
  return files.some(isOriginalUnavailable);
}
async function requireOriginalFiles(files: any[]) {
  if (files.some(isOriginalUnavailable)) {
    toast.warning(t('offline.requires_original'));
    return false;
  }
  const libraryId = libConfig._libraryId;
  for (let i = 0; i < files.length; i += 8) {
    const results = await Promise.all(files.slice(i, i + 8).map(async file => {
      if (!file?.file_path) return false;
      try {
        const available = await checkFileAccessibility(file.file_path);
        if (libraryId === libConfig._libraryId) setFileAccessibility(file.file_path, available);
        return available;
      } catch { return false; }
    }));
    if (libraryId !== libConfig._libraryId) return false;
    if (results.some(available => !available)) {
      toast.warning(t('offline.requires_original'));
      return false;
    }
  }
  return true;
}
async function requireCurrentOriginals() {
  const files = selectMode.value ? await getActionableSelectedItemsForAction() : [fileList.value[selectedItemIndex.value]].filter(Boolean);
  return !!files?.length && await requireOriginalFiles(files);
}

function clickRename() {
  if (currentOriginalsUnavailable()) return;
  // Skip while an inline input is active (e.g. FileInfo rename, where Enter confirms the edit).
  if (uiStore.inputStack.length > 0) return;
  if (selectMode.value) return;
  renamingFileName.value = extractFileName(fileList.value[selectedItemIndex.value].name);
  showRenameMsgbox.value = true;
}

async function clickSetAlbumCover() {
  const file = fileList.value[selectedItemIndex.value];
  const albumId = libConfig.album.id || file?.album_id;
  
  if (file && albumId) {
    try {
      const result = await setAlbumCover(albumId, file.id);
      if (result === null) throw new Error('Failed to update album cover');
      await tauriEmit('album-cover-changed', { albumId: albumId, fileId: file.id });
      toast.success(localeMsg.value.tooltip.set_album_cover.success);
    } catch (error) {
      toast.error(localeMsg.value.tooltip.set_album_cover.failed);
    }
  }
}

async function clickSetDesktopWallpaper() {
  const file = fileList.value[selectedItemIndex.value];
  if (!file?.file_path || !await requireOriginalFiles([file])) return;
  const companionPath = file.media_subtype === 'raw_jpeg_pair'
    ? String(file.live_photo_video_path || '')
    : null;
  try {
    await setDesktopWallpaper(file.file_path, companionPath || null);
    toast.success(localeMsg.value.tooltip.set_desktop_wallpaper.success);
  } catch (error) {
    console.error('Failed to set desktop wallpaper:', error);
    toast.error(localeMsg.value.tooltip.set_desktop_wallpaper.failed);
  }
}

let peopleEditDisposed = false;
let stopPeopleEdits: (() => void) | null = null;
onMounted(async () => {
  const stop = await listen('face-person-changed', (event: any) => {
    const change = event.payload;
    if (peopleEditDisposed || change.library_id !== libConfig._libraryId) return;
    if (change.mode === 'incremental') {
      if (faceChangeAffectsPerson(change, Number(libConfig.person.id)) && (tempViewMode.value === 'person' || (config.main.sidebarIndex === SIDEBAR.PERSON && libConfig.activePane === 'main'))) {
        // Refresh the same person query, not the navigation context; retain random ordering and selection anchors.
        rememberFocusedFileForPresentationRefresh();
        captureSelectionForFileListRefresh();
        currentThumbRequestId++;
        const requestId = ++currentContentRequestId;
        void getFileList({ ...currentQueryParams.value, personId: libConfig.person.id }, requestId, null, currentQueryParams.value.randomSeed);
      }
      return;
    }
    if (change.mode === 'merge' && (change.sourcePersonIds || []).includes(libConfig.person.id)) {
      libConfig.person.name = change.name; libConfig.person.id = change.personId;
      if (tempViewMode.value === 'person') void enterPersonTempView(change.personId, change.name || '');
      return;
    }
    if (isFaceRename(change)) {
      if (libConfig.person.id === change.personId) {
        libConfig.person.name = change.name;
        if (tempViewMode.value === 'person' || (config.main.sidebarIndex === SIDEBAR.PERSON && libConfig.activePane === 'main')) contentTitle.value = change.name || localeMsg.value.sidebar.people;
      }
      return; // Preserve pixels, zoom, selection, scroll and the temporary person view.
    }
    if (faceChangeAffectsPerson(change, Number(libConfig.person.id)) && (tempViewMode.value === 'person' || (config.main.sidebarIndex === SIDEBAR.PERSON && libConfig.activePane === 'main'))) {
      rememberFocusedFileForPresentationRefresh();
      void updateContent(true);
    }
  });
  if (peopleEditDisposed) stop(); else stopPeopleEdits = stop;
});
onBeforeUnmount(() => { peopleEditDisposed = true; stopPeopleEdits?.(); });

async function recognizeSelectedText(force = false) {
  const libraryId = libConfig._libraryId;
  try {
    const files = selectMode.value ? await getActionableSelectedItemsForAction() : [fileList.value[selectedItemIndex.value]].filter(Boolean);
    if (libraryId !== libConfig._libraryId) return;
    if (!files.length || files.some(file => ![1, 3].includes(Number(file.file_type)))) throw new Error(t('ocr.select_photos'));
    if (force && !await ask(t('ocr.reidentify_confirm'), { kind: 'warning' })) return;
    if (libraryId !== libConfig._libraryId) return;
    const receipt = await startOcr({ fileIds: files.map(file => Number(file.id)), force });
    if (libraryId === libConfig._libraryId) toast.success(t(receipt?.queued ? 'recognition_status.priority_queued' : 'ocr.task_started'));
  } catch (e) { if (libraryId === libConfig._libraryId) toast.error(String(e)); }
}
async function recognizeSelectedObjects(force = false) {
  const libraryId=libConfig._libraryId;
  try {
    const files=selectMode.value ? await getActionableSelectedItemsForAction() : [fileList.value[selectedItemIndex.value]].filter(Boolean);
    const ids=objectSelectionIds(files);
    if (files.some(isOriginalUnavailable)) throw new Error(t('face_actions.offline'));
    if(force && !await ask(t('objects.rebuild_confirm'),{kind:'warning'}))return;
    if(libraryId!==libConfig._libraryId)return;
    const receipt = await startObjectIndex({fileIds:ids,force});if(libraryId===libConfig._libraryId)toast.success(t(receipt?.queued?'recognition_status.priority_queued':'objects.started'));
  } catch(error:any){toast.error(error?.message||String(error));}
}
async function rebuildSelectedThumbnails() {
  const libraryId = libConfig._libraryId;
  try {
    const files = selectMode.value ? await getActionableSelectedItemsForAction() : [fileList.value[selectedItemIndex.value]].filter(Boolean);
    if (libraryId !== libConfig._libraryId) return;
    if (files.some(isOriginalUnavailable)) throw new Error(t('thumbnail_rebuild.offline'));
    thumbnailRebuildRequest.value = { libraryId, scope: thumbnailSelectionScope(files) };
  } catch (error: any) { toast.error(error?.message || String(error)); }
}
async function detectSelectedFaces(force = false) {
  const libraryId = libConfig._libraryId;
  try {
    const files = selectMode.value ? await getActionableSelectedItemsForAction() : [fileList.value[selectedItemIndex.value]].filter(Boolean);
    if (libraryId !== libConfig._libraryId) return;
    const ids = faceSelectionIds(files);
    if (files.some(isOriginalUnavailable)) throw new Error(t('face_actions.offline'));
    if (force && !await ask(t('face_actions.redetect_confirm'), { kind: 'warning' })) return;
    if (libraryId !== libConfig._libraryId) return;
    config.setFaceEnabled(true);
    config.settings.face.showBoxes = true;
    const receipt = await indexFaces(ids, force, libraryId);
    if (libraryId === libConfig._libraryId) toast.info(t(receipt?.queued ? 'recognition_status.priority_queued' : 'face_actions.started', { count: ids.length }));
  } catch (error: any) { toast.error(error?.message || String(error)); }
}
function handleItemAction(payload: { action: string, index: number }) {
  if (isSlideShow.value) return;

  const { action, index } = payload;
  if (index >= 0) selectedItemIndex.value = index; // Panel actions have no thumbnail index.
  if (currentOriginalsUnavailable() && requiresOriginalAction(action)) return;

  if(action==='hide-media'){const ids=selectMode.value?Array.from(selectedFileIds):[Number(fileList.value[selectedItemIndex.value]?.id)].filter(id=>id>0);if(ids.length){pendingHide.value={libraryId:libConfig._libraryId,ids};showHideMsgbox.value=true;}return;}
  if (action === 'regenerate-thumbnails') { void rebuildSelectedThumbnails(); return; }

  if (action.startsWith('rating-')) {
    const rating = Number.parseInt(action.slice('rating-'.length), 10);
    if (!Number.isNaN(rating)) {
      void setSelectedFileRating(rating);
    }
    return;
  }

  if (action.startsWith('culling-')) {
    const cullingFlag = action === 'culling-pick'
      ? 1
      : action === 'culling-reject'
        ? 2
        : action === 'culling-unreviewed'
          ? 0
          : -1;
    if (cullingFlag >= 0) {
      if (selectMode.value) {
        void selectModeSetCullingFlags(cullingFlag);
      } else {
        void setSelectedFileCullingFlag(cullingFlag);
      }
    }
    return;
  }
  if (action.startsWith('open-external-app:')) {
    void openInExternalApp(action.slice('open-external-app:'.length));
    return;
  }
  if (action === 'manage-external-apps') {
    showExternalAppsDialog.value = true;
    return;
  }

  const actionMap = {
    'open': () => openImageViewer(selectedItemIndex.value, true),
    'print': () => void printImage(selectedItemIndex.value),
    'edit': () => void openImageEditor(selectedItemIndex.value),
    'open-external-app': () => {
      void openInExternalApp();
    },
    'compare-selected-images': () => {
      const files = getActionableSelectedItems().filter(item => item.file_type === 1 || item.file_type === 3);
      if (files.length < 2) return;
      void openImageViewer(0, true, false, {
        files,
        forceSplitCount: files.length === 2 ? 2 : 4,
      });
    },
    'create-montage': () => void openMontage(),
    'copy': () => void clickCopyImages(fileList.value[selectedItemIndex.value]),
    'rename': clickRename,
    'move-within-library': () => showMoveTo.value = true,
    'move-to-folder': () => void onMoveToFolder(),
    'copy-to-folder': () => void onCopyToFolder(),
    'trash': () => openTrashMsgbox(),
    'remove-from-collection': () => {
      if (currentQuerySource.value === 'collection' && currentCollectionId.value) {
        void removeFileFromCurrentCollection(selectedItemIndex.value);
      }
    },
    'reveal': () => revealPath(fileList.value[selectedItemIndex.value].file_path),
    'refresh-file-info': () => selectMode.value ? startSelectedFileRefresh() : void updateFile(fileList.value[selectedItemIndex.value], true),
    'save-xmp': async () => {
      const ids = selectMode.value
        ? Array.from(selectedFileIds)
        : [fileList.value[selectedItemIndex.value]?.id].filter(Boolean);
      if (!ids.length) return;
      try {
        const result = await saveXmpMetadata(ids, libConfig._libraryId);
        if (result.savedIds.length) toast.success(t('menu.file.xmp_save_success', { count: result.savedIds.length }));
        if (result.failures.length) {
          toast.error(t('menu.file.xmp_save_failed', { count: result.failures.length }) + ': ' + result.failures[0].error);
        }
      } catch (error) { toast.error(String(error)); }
    },
    'favorite': toggleFavorite,
    'rotate': clickRotate,
    'info': toggleInfoPanel,
    'tag': clickTag,
    'add-to-collection': clickAddToCollection,
    'comment': () => showCommentMsgbox.value = true,
    'search-similar': () => enterSimilarSearchMode(fileList.value[selectedItemIndex.value]),
    'recognize-objects': () => void recognizeSelectedObjects(),
    'rerecognize-objects': () => void recognizeSelectedObjects(true),
    'recognize-text': () => void recognizeSelectedText(),
    'rerecognize-text': () => void recognizeSelectedText(true),
    'detect-faces': () => void detectSelectedFaces(),
    'redetect-faces': () => void detectSelectedFaces(true),
    'toggle-face-boxes': () => { config.settings.face.showBoxes = config.settings.face.showBoxes === false; },
    'find-person': () => {
      if (!config.settings.face.enabled) return;
      enterPersonSearchMode(fileList.value[selectedItemIndex.value]);
    },
    'set-album-cover': clickSetAlbumCover,
    'set-desktop-wallpaper': () => void clickSetDesktopWallpaper(),
  };

  if ((actionMap as any)[action]) {
    // Check for unsaved changes before performing action, especially if it might change the context
    // Most actions here operate on `index` which becomes the selected index. 
    // If index is different from current selectedItemIndex, we should check.
    
    if (index !== selectedItemIndex.value) {
        checkUnsavedChanges(() => {
             (actionMap as any)[action]();
        });
    } else {
         (actionMap as any)[action]();
    }
  }
}

function requestNavigate(direction: 'prev' | 'next') {
  checkUnsavedChanges(() => {
    const viewer = showQuickView.value ? quickViewMediaRef.value : (isFilmstripView.value ? filmStripMediaRef.value : null);
    
    if (direction === 'next') {
      if (viewer) {
        viewer.triggerNext();
      } else {
        performNavigate('next');
      }
    } else {
      if (viewer) {
        viewer.triggerPrev();
      } else {
        performNavigate('prev');
      }
    }
  });
}

function performNavigate(direction: 'prev' | 'next') {
  checkUnsavedChanges(() => {
    if (direction === 'next') {
      if (selectedItemIndex.value < fileList.value.length - 1) {
        selectedItemIndex.value += 1;
      }
    } else {
      if (selectedItemIndex.value > 0) {
        selectedItemIndex.value -= 1;
      }
    }
  });
}

function updateScrollPosition(currentScrollTop: number, currentScrollHeight: number) {
    if (!isFilmstripView.value) {
      // Calculate max scroll top
      const totalRows = Math.ceil(scrollbarTotal.value / columnCount.value);
      const topPadding = 48;
      const bottomPadding = config.settings.showStatusBar ? 32 : 4;
      
      // Determine effective scroll height
      // If provided (from event), use it. Otherwise calculate based on layout.
      let scrollHeight = currentScrollHeight;
      if (!scrollHeight) {
         const contentHeight = layoutContentHeight.value > 0
            ? layoutContentHeight.value 
            : (totalRows * itemSize.value);
         scrollHeight = contentHeight + topPadding + bottomPadding;
      }

      const maxScrollTop = Math.max(0, scrollHeight - containerHeight.value);
      
      if (maxScrollTop <= 0) {
        scrollPosition.value = 0;
      } else {
        const ratio = Math.min(1, Math.max(0, currentScrollTop / maxScrollTop));
        const maxIndex = Math.max(1, scrollbarTotal.value - scrollbarPageSize.value);
        scrollPosition.value = Math.round(ratio * maxIndex);
      }
    } else if (isFilmstripView.value) {
      // Fallback for filmstrip or other layouts (horizontal)
      const rowIndex = Math.floor(currentScrollTop / itemSize.value);
      scrollPosition.value = rowIndex * columnCount.value;
    }
}

function handleGridScroll(event: any) {
  if (event && event.target) {
    updateScrollPosition(
        event.target.scrollTop, 
        event.target.scrollHeight
    );
  }
}

function handleLayoutUpdate({ height }: { height: number }) {
  layoutContentHeight.value = height;
  if (gridViewRef.value) {
    updateScrollPosition(gridViewRef.value.getScrollTop(), 0);
  }
}

function markDedupSourceUpdated(requestId?: number) {
  if (requestId !== undefined && requestId !== currentContentRequestId) {
    return;
  }
  dedupSourceVersion.value += 1;
}

function handleScrollUpdate(newIndex: number) {
  scrollPosition.value = newIndex;
  
  if (!isFilmstripView.value && gridViewRef.value) {
    // Calculate ratio (0 to 1)
    const maxIndex = Math.max(1, scrollbarTotal.value - scrollbarPageSize.value);
    const ratio = Math.min(1, Math.max(0, newIndex / maxIndex));
    
    // Calculate max scroll top
    const totalRows = Math.ceil(scrollbarTotal.value / columnCount.value);
    const topPadding = 48;
    const bottomPadding = config.settings.showStatusBar ? 32 : 4;
    
    // Use reported layout height when available; date headers also affect normal grid height.
    const contentHeight = layoutContentHeight.value > 0
      ? layoutContentHeight.value 
      : (totalRows * itemSize.value);
      
    const totalHeight = contentHeight + topPadding + bottomPadding;
    const maxScrollTop = Math.max(0, totalHeight - containerHeight.value);
    
    const newScrollTop = ratio * maxScrollTop;
    
    gridViewRef.value.scrollToPosition(newScrollTop);
  } else if (gridScrollContainerRef.value) {
    // For filmstrip or other layouts, fallback to simple calculation or existing logic
    // But filmstrip uses horizontal scroll, handled differently.
    // This block might be for fallback.
    const rowIndex = Math.floor(newIndex / columnCount.value);
    const newScrollTop = rowIndex * itemSize.value;
    gridScrollContainerRef.value.scrollTop = newScrollTop;
  }
}

// Keyboard navigation actions
const keyActions = {
  ArrowDown: () => {
    if (getActivePreviewMode() !== 'none') return;
    checkUnsavedChanges(() => {
      if (gridViewRef.value) {
        if (usesGeometryNavigation.value) {
          // Use geometry-aware navigation for variable-sized layouts.
          const nextIndex = gridViewRef.value.getNextItemIndex(selectedItemIndex.value, 'down');
          selectedItemIndex.value = nextIndex !== -1 ? nextIndex : selectedItemIndex.value;
        } else {
          selectedItemIndex.value = Math.min(selectedItemIndex.value + gridViewRef.value.getColumnCount(), fileList.value.length - 1);
        }
      }
    });
  },
  ArrowUp: () => {
    if (getActivePreviewMode() !== 'none') return;
    checkUnsavedChanges(() => {
      if (gridViewRef.value) {
        if (usesGeometryNavigation.value) {
          // Use geometry-aware navigation for variable-sized layouts.
          const nextIndex = gridViewRef.value.getNextItemIndex(selectedItemIndex.value, 'up');
          selectedItemIndex.value = nextIndex !== -1 ? nextIndex : selectedItemIndex.value;
        } else {
          selectedItemIndex.value = Math.max(selectedItemIndex.value - gridViewRef.value.getColumnCount(), 0);
        }
      }
    });
  },
  Home: () => {
    checkUnsavedChanges(() => {
      selectedItemIndex.value = 0;
    });
  },
  End: () => {
    checkUnsavedChanges(() => {
      selectedItemIndex.value = fileList.value.length - 1;
    });
  },
};

// Local keydown handler for navigation (prevents default browser behavior)
function handleLocalKeyDown(event: KeyboardEvent) {
  if (event.defaultPrevented) return;

  // Do not intercept editing shortcuts while typing.
  const target = event.target as HTMLElement;
  if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) {
    return;
  }

  // Check if there are modal dialogs
  if (uiStore.inputStack.length > 0) {
    return;
  }

  if (matchesShortcut('file.paste', event, shortcutPlatform)) {
    event.preventDefault();
    void pasteClipboardImage();
    return;
  }

  if (uiStore.activePane === 'left-sidebar') {
    return;
  }

  if (isMapVisible.value && (event.key === 'Space' || event.key === ' ')) {
    event.preventDefault();
    return;
  }

  if (event.key === 'Shift') {
    if (selectedItemIndex.value >= 0) {
      keyboardSelectionAnchorIndex.value = selectedItemIndex.value;
    }
    return;
  }

  if (selectMode.value && matchesShortcut('file.selectNone', event, shortcutPlatform)) {
    event.preventDefault();
    selectNoneInCurrentList();
    return;
  }

  if (selectMode.value && matchesShortcut('file.invertSelection', event, shortcutPlatform)) {
    event.preventDefault();
    void invertSelectionInCurrentList();
    return;
  }

  if (matchesShortcut('meta.info', event, shortcutPlatform)) {
    event.preventDefault();
    toggleInfoPanel();
    return;
  }

  if (showQuickView.value && (event.key === ' ' || event.key === 'Space') && matchesShortcut('view.quickPreview', event, shortcutPlatform)) {
    event.preventDefault();
    // macOS dispatches this shortcut through the capture/global channel.
    if (!isMac && !event.repeat) closeQuickPreview();
    return;
  }

  if (matchesShortcut('view.close', event, shortcutPlatform)) {
    // Close Quick Preview in one step, even in fullscreen. Unmount restores
    // the native window while preserving the saved fullscreen preference.
    if (showQuickView.value) {
      event.preventDefault();
      closeQuickPreview();
      return;
    }
    // Filmstrip is an embedded layout, so Escape returns to that layout.
    const preview = getActivePreviewMediaRef();
    if (preview?.isFullScreen) {
      event.preventDefault();
      void preview.exitPreviewFullScreen();
      return;
    }
    if (selectMode.value) {
      if (selectionPanelOpen.value) config.rightPanel.show = false;
      handleSelectMode(false);
      event.preventDefault();
      return;
    }
    if (tempViewMode.value !== 'none') {
      exitTempViewMode();
      event.preventDefault();
      return;
    } else if (config.rightPanel.show) {
      config.rightPanel.show = false;
      event.preventDefault();
      return;
    }
  }

  if (matchesShortcut('file.selectAll', event, shortcutPlatform)) {
    event.preventDefault();
    void selectAllInCurrentList();
    return;
  }

  if (selectedItemIndex.value < 0 || fileList.value.length === 0) {
    return;
  }

  // Keep only close and background shortcuts available during slideshow.
  if (
    isSlideShow.value &&
    !matchesShortcut('view.close', event, shortcutPlatform) &&
    !matchesShortcut('view.cycleBackground', event, shortcutPlatform) &&
    getMatchedViewBackground(event) === null
  ) {
    return;
  }

  const ratingShortcut = getMatchedRating(event);
  if (ratingShortcut !== null) {
    event.preventDefault();
    if (selectMode.value && selectedCount.value === 0) return;
    if (selectMode.value) {
      void selectModeSetRatings(ratingShortcut);
    } else {
      if (!event.repeat) void setSelectedFileRating(ratingShortcut, event.shiftKey);
    }
    return;
  }

  const cullingShortcut = getMatchedCulling(event);
  if (cullingShortcut !== null) {
    event.preventDefault();
    if (selectMode.value && selectedCount.value === 0) return;
    if (selectMode.value) {
      void selectModeSetCullingFlags(cullingShortcut);
    } else {
      if (!event.repeat) void setSelectedFileCullingFlag(cullingShortcut, event.shiftKey);
    }
    return;
  }

  if (matchesShortcut('file.searchSimilar', event, shortcutPlatform)) {
    event.preventDefault();
    if (selectMode.value && selectedCount.value === 0) return;
    enterSimilarSearchMode(fileList.value[selectedItemIndex.value]);
    return;
  }

  if (matchesShortcut('file.openExternalApp', event, shortcutPlatform)) {
    event.preventDefault();
    if (selectMode.value && selectedCount.value === 0) return;
    void openInExternalApp();
    return;
  }

  if (matchesShortcut('file.reveal', event, shortcutPlatform)) {
    event.preventDefault();
    if (selectMode.value && selectedCount.value === 0) return;
    if (!currentOriginalsUnavailable()) revealPath(fileList.value[selectedItemIndex.value].file_path);
    return;
  }

  if (getActivePreviewMode() !== 'none' && matchesShortcut('view.cycleBackground', event, shortcutPlatform)) {
    event.preventDefault();
    config.cycleViewBackground();
    void tauriEmit('settings-viewBackground-changed', config.settings.viewBackground);
    return;
  }

  const viewBackground = getMatchedViewBackground(event);
  if (getActivePreviewMode() !== 'none' && viewBackground !== null) {
    event.preventDefault();
    setPreviewViewBackground(viewBackground);
    return;
  }


  if (getActivePreviewMode() !== 'none' && matchesShortcut('view.zoomFit', event, shortcutPlatform)) {
    event.preventDefault();
    if (!event.repeat) {
      if (getActivePreviewMode() === 'quick-view') {
        quickViewZoomFit.value = !quickViewZoomFit.value;
      } else {
        filmStripZoomFit.value = !filmStripZoomFit.value;
      }
    }
    return;
  }

  if (getActivePreviewMode() !== 'none' && matchesShortcut('view.zoomIn', event, shortcutPlatform) && event.key === '=') {
    event.preventDefault();
    getActivePreviewMediaRef()?.zoomIn?.();
    return;
  }

  if (getActivePreviewMode() !== 'none' && matchesShortcut('view.zoomOut', event, shortcutPlatform) && event.key === '-') {
    event.preventDefault();
    getActivePreviewMediaRef()?.zoomOut?.();
    return;
  }

  if (getActivePreviewMode() === 'none' && matchesShortcut('view.zoomIn', event, shortcutPlatform) && event.key === '=') {
    if (!isContentInteractionActive()) return;
    event.preventDefault();
    setGridSize(gridSize.value + 10);
    return;
  }

  if (getActivePreviewMode() === 'none' && matchesShortcut('view.zoomOut', event, shortcutPlatform) && event.key === '-') {
    if (!isContentInteractionActive()) return;
    event.preventDefault();
    setGridSize(gridSize.value - 10);
    return;
  }

  if (matchesShortcut('meta.favorite', event, shortcutPlatform)) {
    event.preventDefault();
    if (selectMode.value) {
      void toggleSelectModeFavorite();
    } else {
      if (!event.repeat) void toggleFavorite(event.shiftKey);
    }
    return;
  }

  if (matchesShortcut('meta.tag', event, shortcutPlatform)) {
    event.preventDefault();
    if (selectMode.value && selectedCount.value === 0) return;
    void clickTag();
    return;
  }

  if (matchesShortcut('meta.collection', event, shortcutPlatform)) {
    event.preventDefault();
    if (selectMode.value && selectedCount.value === 0) return;
    void clickAddToCollection();
    return;
  }

  if (matchesShortcut('meta.comment', event, shortcutPlatform)) {
    event.preventDefault();
    if (selectMode.value && selectedCount.value === 0) return;
    showCommentMsgbox.value = true;
    return;
  }

  if (matchesShortcut('meta.rotate', event, shortcutPlatform)
    || matchesShortcut('meta.rotateCounterclockwise', event, shortcutPlatform)) {
    event.preventDefault();
    if (selectMode.value && selectedCount.value === 0) return;
    void rotateSelected(event.shiftKey ? -90 : 90);
    return;
  }

  if (matchesShortcut('file.moveTo', event, shortcutPlatform)) {
    event.preventDefault();
    if (selectMode.value && selectedCount.value === 0) return;
    if (!currentOriginalsUnavailable()) showMoveTo.value = true;
    return;
  }

  if (matchesShortcut('file.moveToFolder', event, shortcutPlatform)) {
    event.preventDefault();
    if (selectMode.value && selectedCount.value === 0) return;
    void onMoveToFolder();
    return;
  }

  if (matchesShortcut('file.rename', event, shortcutPlatform)) {
    event.preventDefault();
    event.stopPropagation();
    clickRename();
    return;
  }

  if (!isMac && matchesShortcut('view.first', event, shortcutPlatform)) {
    event.preventDefault();
    checkUnsavedChanges(() => {
      selectedItemIndex.value = 0;
    });
    return;
  }

  if (!isMac && matchesShortcut('view.last', event, shortcutPlatform)) {
    event.preventDefault();
    checkUnsavedChanges(() => {
      selectedItemIndex.value = fileList.value.length - 1;
    });
    return;
  }

  if (matchesShortcut('view.pageUp', event, shortcutPlatform) || matchesShortcut('view.pageDown', event, shortcutPlatform)) {
    if (getActivePreviewMode() === 'none') event.preventDefault();
    return; // Navigation is dispatched once through global-keydown below.
  }

  const handledKeys = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Home', 'End', 'Enter', 'Space', ' '];

  if (!isMac) {
    handleQuickPreviewShortcut(event);
  }

  if (handledKeys.includes(event.key)) {
    event.preventDefault();
  }

  const isFilmstrip = isFilmstripView.value;
  const selectionDirection =
    event.key === 'ArrowRight' || (isFilmstrip && event.key === 'ArrowDown')
      ? 'next'
      : event.key === 'ArrowLeft' || (isFilmstrip && event.key === 'ArrowUp')
        ? 'prev'
        : null;

  if (event.shiftKey && selectionDirection && toggleKeyboardSelection(selectionDirection)) {
    return;
  }

  if (event.key === 'ArrowRight' || (isFilmstrip && event.key === 'ArrowDown')) {
    requestNavigate('next');
  } else if (event.key === 'ArrowLeft' || (isFilmstrip && event.key === 'ArrowUp')) {
    requestNavigate('prev');
  }
}

function handleQuickPreviewShortcut(event: { key: string; code?: string; repeat?: boolean }): boolean {
  if (!matchesShortcut('view.quickPreview', event, shortcutPlatform)) {
    return false;
  }
  if (event.repeat) return true;

  if (event.key === 'Enter') {
    if (!showQuickView.value && !isFilmstripView.value) {
      showQuickView.value = true;
      quickViewZoomFit.value = true;
    }
    return true;
  }

  if (event.key !== 'Space' && event.key !== ' ') {
    return false;
  }

  if (getActivePreviewMode() === 'quick-view') {
    closeQuickPreview();
  } else if (!isFilmstripView.value) {
    showQuickView.value = true;
    quickViewZoomFit.value = true;
  }

  return true;
}

function handleLocalKeyUp(event: KeyboardEvent) {
  if (event.key === 'Shift') {
    keyboardSelectionAnchorIndex.value = -1;
  }
}

function isContentInteractionActive() {
  return isContentHovered.value && !uiStore.mapActive;
}

function activateContentPane(event?: Event) {
  uiStore.setActivePane('content');
  // Don't steal focus from an editable field: this mousedown.capture runs before the field's own
  // handlers, so focusing contentRoot would blur it (e.g. abort FileInfo's rename on a mere click).
  const target = event?.target as HTMLElement | null;
  if (target?.closest?.('input, textarea, [contenteditable="true"]')) return;
  contentRootRef.value?.focus({ preventScroll: true });
}

function restoreOpenPreviewFocus() {
  if (uiStore.inputStack.length > 0) return;
  activateContentPane();
}

function handleContentWheel(event: WheelEvent) {
  if (!event.ctrlKey) return;
  if (getActivePreviewMode() !== 'none') return;
  if (!isContentInteractionActive()) return;

  event.preventDefault();
  event.stopPropagation();

  const currentSize = gridSize.value;
  const delta = event.deltaY < 0 ? 10 : -10;
  setGridSize(currentSize + delta);
}

function isTextInputFocused() {
  const active = document.activeElement as HTMLElement | null;
  if (!active) return false;
  return active.tagName === 'INPUT' || active.tagName === 'TEXTAREA' || active.isContentEditable;
}

// Global keydown handler (from Tauri)
const handleKeyDown = (e: any) => {
  if (uiStore.activePane === 'left-sidebar') {
    return;
  }

  if (isTextInputFocused()) {
    return;
  }

  if (uiStore.inputStack.length > 0) {
    return;
  }

  const event = e.payload;
  const { key } = event;

  if (matchesShortcut('view.pageUp', event, shortcutPlatform) || matchesShortcut('view.pageDown', event, shortcutPlatform)) {
    if (getActivePreviewMode() !== 'none' || isSlideShow.value || selectedItemIndex.value < 0) return;
    checkUnsavedChanges(() => {
      const direction = matchesShortcut('view.pageUp', event, shortcutPlatform) ? 'up' : 'down';
      const nextIndex = gridViewRef.value?.getNextItemIndex(selectedItemIndex.value, direction, true);
      if (nextIndex !== undefined && nextIndex >= 0) selectedItemIndex.value = nextIndex;
    });
    return;
  }

  if (isMapVisible.value && (key === 'Space' || key === ' ')) {
    return;
  }

  // Disable global shortcuts during slideshow except close for safety.
  if (isSlideShow.value && !matchesShortcut('view.close', event, shortcutPlatform) &&
    !(showQuickView.value && (key === ' ' || key === 'Space') && matchesShortcut('view.quickPreview', event, shortcutPlatform))) {
    return;
  }

  // GridView prevents Space's default behavior before Content's window listener on macOS,
  // so Quick Preview uses the App capture/global channel there.
  if (isMac && handleQuickPreviewShortcut(event)) {
    return;
  }

  if (matchesShortcut('file.openNewWindow', event, shortcutPlatform)) {
    if (selectMode.value && selectedCount.value === 0) return;
    openImageViewer(selectedItemIndex.value, true);
  } else if (
    isMac &&
    selectedItemIndex.value >= 0 &&
    selectedItemIndex.value < fileList.value.length &&
    fileList.value.length > 0 &&
    matchesShortcut('file.rename', event, shortcutPlatform)
  ) {
    // GridView may consume Enter before Content's DOM keydown handler on macOS.
    // Handle the shortcut in the capture/global channel as a fallback.
    clickRename();
  } else if (matchesShortcut('file.copy', event, shortcutPlatform)) {
    if (selectMode.value && selectedCount.value === 0) return;
    void clickCopyImages(fileList.value[selectedItemIndex.value]);
  // macOS handles Cmd+Arrow in App's capture listener before the content DOM handler.
  } else if (isMac && matchesShortcut('view.first', event, shortcutPlatform)) {
    keyActions.Home();
  } else if (isMac && matchesShortcut('view.last', event, shortcutPlatform)) {
    keyActions.End();
  } else if (matchesShortcut('file.editImage', event, shortcutPlatform)) {
    if (selectMode.value && selectedCount.value === 0) return;
    const file = fileList.value[selectedItemIndex.value];
    if (file && (file.file_type === 1 || file.file_type === 3)) {
      void openImageEditor(selectedItemIndex.value);
    }
  } else if (matchesShortcut('file.trash', event, shortcutPlatform)) {
    if (selectMode.value && selectedCount.value === 0) return;
    openTrashMsgbox();
  } else if (key === 'ArrowUp' || key === 'ArrowDown') {
    (keyActions as any)[key]();
  }
};

// --- Indexing Logic ---
let unlistenIndexProgress: (() => void) | undefined;
let unlistenIndexFinished: (() => void) | undefined;
let unlistenThumbnailReady: (() => void) | undefined;
let unlistenTriggerNextAlbum: (() => void) | undefined;
let unlistenRefreshContent: (() => void) | undefined;
let unlistenFilesDeleted: (() => void) | undefined;
let unlistenAlbumUpdated: (() => void) | undefined;
const showIndexRecoveryMsgbox = ref(false);
const recoverySkipFilePath = ref('');  // local: file path from crash trace
const indexRecoveryTitle = computed(() => localeMsg.value.search.index.recovery.title);
const indexRecoveryOkText = computed(() => localeMsg.value.search.index.recovery.continue);
const indexRecoverySkipLabel = computed(() => localeMsg.value.search.index.recovery.skip_once);
const indexRecoveryFileLabel = computed(() => localeMsg.value.search.index.recovery.file_label);
const indexRecoveryMessage = computed(() => {
  return localeMsg.value.search.index.recovery.message;
});

const scanHistoryScope=ref<ScanScope|null>(null);
const scanHistoryStops:Array<()=>void>=[];
const activeScanRunId=computed({get:()=>String(libConfig.index.runId||''),set:(value:string)=>{libConfig.index.runId=value;}});
const scanPresentationDeferred=ref<{libraryId:string;albumId:number}|null>(null);
function scanHasPendingEdits(){return hasUnsavedChanges.value || pendingChanges.hasPending();}
function deferScanPresentation(albumId=Number(libConfig.album.id)){scanPresentationDeferred.value={libraryId:libConfig._libraryId,albumId};}
function flushScanPresentation(){
  const scope=scanPresentationDeferred.value;if(!scope)return;
  if(scope.libraryId!==libConfig._libraryId||config.main.sidebarIndex!==SIDEBAR.ALBUM||Number(libConfig.album.id)!==scope.albumId){scanPresentationDeferred.value=null;return;}
  if(scanHasPendingEdits())return;
  scanPresentationDeferred.value=null;rememberFocusedFileForPresentationRefresh();captureSelectionForFileListRefresh();void updateContent(true);
}
watch([()=>uiStore.inputStack.length,hasUnsavedChanges,()=>libConfig._libraryId,()=>config.main.sidebarIndex,()=>libConfig.album.id],flushScanPresentation);
let scanStarting=false;
useScanTracking();
async function processNextAlbum(skipFilePath: string | null = null, skipRecoveryCheck = false) {
  if (scanStarting || libConfig._initialized === false || Number(libConfig.index.status)!==1) return;
  const library=libConfig._libraryId, albumId=Number(libConfig.index.albumQueue[0]||0);
  if (!albumId) { syncIndexStatus(); return; }
  if(scanJobs.pausing.has(albumId))return;
  const backend=albumScanRun(albumId);
  if (backend?.retry?.running) return;
  if (backend && ['running','pausing'].includes(backend.status)) {activeScanRunId.value=backend.id;return;}
  if(scanHasPendingEdits()){libConfig.index.status=2;toast.info(t('scan_tracking.save_edits_first'));return;}
  const runId=crypto.randomUUID();activeScanRunId.value=runId;scanStarting=true;
  const current=()=>library===libConfig._libraryId && !scanJobs.pausing.has(albumId) && libConfig._initialized!==false && Number(libConfig.index.status)===1 && Number(libConfig.index.albumQueue[0])===albumId;
  try {
    scanProgressSession.value++;Object.assign(libConfig.index,{albumName:'',phase:'discovering',discovered:0,processed:0,searchReady:0,indexed:0,total:0,searchTotal:0,failed:0});
    const album=await getAlbum(albumId);if(!current())return;
    if(!album){libConfig.index.albumQueue.shift();return;}
    if(!skipFilePath&&!skipRecoveryCheck){
      const recoveryInfo=await getIndexRecoveryInfo();if(!current())return;
      if(recoveryInfo&&Number(recoveryInfo.album_id)===albumId){recoverySkipFilePath.value=String(recoveryInfo.file_path||'');libConfig.index.status=2;showIndexRecoveryMsgbox.value=true;return;}
    }
    libConfig.index.pausedAlbumIds=libConfig.index.pausedAlbumIds.filter((id:any)=>Number(id)!==albumId);
    Object.assign(libConfig.index,{albumName:album.name,phase:'discovering',discovered:0,processed:0,searchReady:0,indexed:0,total:0,searchTotal:0,failed:0});
    if(scanHasPendingEdits()){libConfig.index.status=2;toast.info(t('scan_tracking.save_edits_first'));return;}
    const run=await indexAlbum(albumId,skipFilePath||null,runId,library);
    if(library===libConfig._libraryId)applyScanEvent({run});
  } catch(error) {
    if(current()){libConfig.index.status=2;toast.error(t('scan_tracking.start_failed',{error:String(error)}));void refreshScanJobs();}
  } finally {
    scanStarting=false;
    if(library===libConfig._libraryId&&!libConfig.index.albumQueue.length)syncIndexStatus();
    if(library===libConfig._libraryId && Number(libConfig.index.status)===1 && libConfig.index.albumQueue.length && Number(libConfig.index.albumQueue[0])!==albumId) void processNextAlbum();
  }
}

// Check if current album is being indexed
const isIndexing = computed(() => {
  return config.main.sidebarIndex === SIDEBAR.ALBUM &&
         libConfig.activePane !== 'collection' &&
         !!libConfig.album.id && libConfig.album.id > 0 && // Valid album
         Number(libConfig.index.albumQueue[0] || 0) === Number(libConfig.album.id) &&
         Number(libConfig.index.status || 0) !== 2;
});

const isAnyIndexing = computed(() =>
  libConfig.index.albumQueue.length > 0 && Number(libConfig.index.status || 0) !== 2
);

const isScanStreamingMode = computed(() =>
  isIndexing.value &&
  config.main.sidebarIndex === SIDEBAR.ALBUM &&
  Boolean(libConfig.album.selected)
);

// When scanning starts, close panels and multi-select which rely on a stable file list.
watch(isScanStreamingMode, (streaming) => {
  if (streaming) {
    if(scanHasPendingEdits()){
      void pauseAlbumScan(Number(libConfig.album.id)).catch(error=>toast.error(String(error)));
      toast.info(t('scan_tracking.save_edits_first'));return;
    }
    config.rightPanel.show = false;
    if (selectMode.value) selectMode.value = false;
  }
  if (!scanHasPendingEdits() && (groupedModeActive.value || effectiveGroupBy.value > 0)) {
    updateContent();
  }
});

const isAlbumPaused = (albumId: number | null | undefined) =>
  (libConfig.index.pausedAlbumIds as any[]).some(id => Number(id) === Number(albumId || 0));
const syncIndexStatus = () => {
  if ((libConfig.index.albumQueue as any[]).length > 0) {
    libConfig.index.status = 1;
  } else if ((libConfig.index.pausedAlbumIds as any[]).length > 0) {
    libConfig.index.status = 2;
  } else {
    libConfig.index.status = 0;
  }
};

watch(()=>Object.values(scanJobs.runs).filter(run=>run.library_id===libConfig._libraryId && ['running','pausing'].includes(run.status)).sort((a,b)=>b.seq-a.seq)[0],run=>{
  if(!run || scanJobs.libraryId!==libConfig._libraryId || libConfig._initialized===false)return;
  if(!libConfig.index.albumQueue.some((id:any)=>Number(id)===run.album_id))libConfig.index.albumQueue.unshift(run.album_id);
  if(Number(libConfig.index.albumQueue[0])===run.album_id){activeScanRunId.value=run.id;libConfig.index.status=1;}
});
const isRecoveryInProgress = ref(false);

const confirmIndexRecoveryContinue = async (shouldSkip = false) => {
  showIndexRecoveryMsgbox.value = false;
  if(scanHasPendingEdits()){toast.info(t('scan_tracking.save_edits_first'));return;}
  const skipFilePath = shouldSkip ? recoverySkipFilePath.value : null;
  recoverySkipFilePath.value = '';
  const library=libConfig._libraryId;
  await clearIndexRecoveryInfo();  // delete trace file first to prevent re-detection
  if(library!==libConfig._libraryId)return;
  isRecoveryInProgress.value = true;
  libConfig.index.status = 1;
  await processNextAlbum(skipFilePath, true);
  isRecoveryInProgress.value = false;
};

const cancelIndexRecovery = () => {
  showIndexRecoveryMsgbox.value = false;
  libConfig.index.status = 2;
};
const activeScanningAlbumId = computed(() => Number(libConfig.index.albumQueue[0] || 0));
const suppressNextIndexingIdleRefresh = ref(false);
const selectedAlbumIdForStatusBar = computed(() => Number(libConfig.album.id || 0));
const selectedAlbumScanState = computed(() => getAlbumScanState({
  run: albumScanRun(selectedAlbumIdForStatusBar.value),
  runId: libConfig.index.runId,
  pausing: scanJobs.pausing.has(selectedAlbumIdForStatusBar.value),
  albumId: selectedAlbumIdForStatusBar.value,
  albumQueue: libConfig.index.albumQueue as any[],
  pausedAlbumIds: libConfig.index.pausedAlbumIds as any[],
  status: Number(libConfig.index.status || 0),
}));
const selectedAlbumScanIcon = computed(() => getAlbumScanIcon(selectedAlbumScanState.value));
const selectedAlbumScanAnimating = computed(() => shouldAnimateAlbumScanIcon(selectedAlbumScanState.value));
const isOtherTabScanning = computed(() =>
  config.main.sidebarIndex !== SIDEBAR.ALBUM &&
  (libConfig.index.albumQueue as any[]).length > 0 &&
  Number(libConfig.index.status || 0) !== 2
);
const showBackgroundScanningIcon = computed(() =>
  !['scanning','pausing','queued','paused'].includes(selectedAlbumScanState.value) &&
  (libConfig.index.albumQueue as any[]).length > 0 &&
  Number(libConfig.index.status || 0) !== 2
);

const statusBarScanMode = computed<'none' | 'current' | 'waiting' | 'background' | 'paused'>(() => {
  if (config.main.sidebarIndex === SIDEBAR.ALBUM) {
    if (selectedAlbumScanState.value === 'scanning') return 'current';
    if (selectedAlbumScanState.value === 'queued') return 'waiting';
    if (selectedAlbumScanState.value === 'paused') return 'paused';
    if (selectedAlbumScanState.value === 'pausing') return 'current';
    if (showBackgroundScanningIcon.value) return 'background';
    return 'none';
  }
  if (isOtherTabScanning.value) return 'background';
  return 'none';
});

const statusBarShowUpdateIcon = computed(() =>
  selectedAlbumScanIcon.value !== 'none' || statusBarScanMode.value === 'background'
);
const statusBarUpdateIcon = computed<'update' | 'dot'>(() =>
  statusBarScanMode.value === 'background' ? 'update' : selectedAlbumScanIcon.value === 'dot' ? 'dot' : 'update'
);
const statusBarIsUpdateAnimating = computed(() =>
  statusBarScanMode.value === 'background' || selectedAlbumScanAnimating.value
);

const activeScanReport = computed(() => {
  const run = albumScanRun(activeScanningAlbumId.value);
  return run && run.id === activeScanRunId.value && ['running','pausing'].includes(run.status) ? run : undefined;
});
const activeScanWork = computed(() => activeScanReport.value ? scanWorkProgress(activeScanReport.value) : null);
const statusBarScanText = computed(() => {
  if(selectedAlbumScanState.value==='pausing')return t('scan_tracking.state_pausing');
  const discovered = Number(libConfig.index.discovered || 0).toLocaleString();
  const processed = Number(libConfig.index.processed || libConfig.index.indexed || 0).toLocaleString();
  const searchReady = Number(libConfig.index.searchReady || 0).toLocaleString();
  const total = Number(libConfig.index.total || 0).toLocaleString();
  const searchTotal = Number(libConfig.index.searchTotal || 0).toLocaleString();
  const phase = String(libConfig.index.phase || 'discovering');

  if (statusBarScanMode.value === 'waiting') {
    return localeMsg.value.search.index.wait_index || 'Wait for scan...';
  }
  if (statusBarScanMode.value === 'paused') {
    return localeMsg.value.search.index.paused || 'Scan paused';
  }
  if (statusBarScanMode.value !== 'current') return '';

  if (activeScanWork.value && activeScanReport.value) {
    const work = activeScanWork.value, counts = activeScanReport.value.counts;
    const progress = ['counting','finalizing','draining'].includes(work.phase) ? '' : `${work.current.toLocaleString()} / ${work.total.toLocaleString()}`;
    return [t('scan_tracking.phase_'+work.phase), progress, counts.pending > 0 ? t('scan_tracking.pending_short',{count:counts.pending}) : ''].filter(Boolean).join(' · ');
  }

  if (phase === 'preparing_previews') {
    return (localeMsg.value.search.index.preparing_previews || 'Generating previews... {count} / {total}')
      .replace('{count}', processed)
      .replace('{total}', total);
  }
  if (phase === 'preparing_search') {
    return (localeMsg.value.search.index.preparing_search || 'Preparing search... {count} / {total}')
      .replace('{count}', searchReady)
      .replace('{total}', searchTotal);
  }
  if (phase === 'complete') {
    if (Number(libConfig.index.failed || 0) > 0) {
      return (localeMsg.value.search.index.complete_with_issues || 'Scan complete with {count} issues')
        .replace('{count}', Number(libConfig.index.failed || 0).toLocaleString());
    }
    return localeMsg.value.search.index.complete || 'Scan complete';
  }

  return (localeMsg.value.search.index.discovering || 'Scanning library... {count} / {total}')
    .replace('{count}', discovered)
    .replace('{total}', total);
});

const topProgressPercent = computed(() => {
  const work = activeScanWork.value;
  const phase = work?.phase || String(libConfig.index.phase || 'discovering');
  const total = work?.total ?? Number(libConfig.index.total || 0);
  const discovered = work?.current ?? Number(libConfig.index.discovered || 0);
  const processed = work?.current ?? Number(libConfig.index.processed || 0);
  const searchReady = work?.current ?? Number(libConfig.index.searchReady || 0);
  const searchTotal = work?.total ?? Number(libConfig.index.searchTotal || 0);
  const ratio = (current: number, max: number) => max > 0
    ? Math.min(1, Math.max(0, current / max))
    : 0;

  if (['finalizing','draining'].includes(phase)) return 99;
  if (phase === 'counting') return 1;
  if (phase === 'preparing_previews') {
    return 46 + Math.round(ratio(processed, total) * 44);
  }
  if (phase === 'preparing_search') {
    return 91 + Math.round(ratio(searchReady, searchTotal) * 8);
  }
  return Math.max(1, Math.round(ratio(discovered, total) * 45));
});

const showTopProgressBar = computed(() =>
  Number(libConfig.index.status) === 1 &&
  (libConfig.index.albumQueue as any[]).length > 0 &&
  (!!activeScanWork.value || String(libConfig.index.phase || '') !== 'complete')
);

function buildScanStreamQueryParams() {
  return {
    searchFileType: 0,
    sortType: 9, // internal sort: by id asc (insert/scan order)
    sortOrder: 0,
    searchFileName: "",
    searchAllSubfolders: libConfig.album.folderPath || "",
    searchFolder: "",
    startDate: 0,
    endDate: 0,
    calendarSort: 0,
    folderSort: 0,
    categorySort: 0,
    make: "",
    model: "",
    lensMake: "",
    lensModel: "",
    locationNodeId: 0,
    isFavorite: false,
    rating: -1,
    tagId: 0,
    tagGroupId: 0,
    personId: 0,
  };
}

async function syncScanStreamingTitle(albumId: number) {
  const album = await getAlbum(albumId);
  if (
    isScanStreamingMode.value &&
    Number(libConfig.album.id) === Number(albumId) &&
    album
  ) {
    contentTitle.value = album.name;
  }
}

function enterScanStreamingMode(albumId: number) {
  if(scanHasPendingEdits())return;
  // Scan order is a new query snapshot: pages from the previous sort or album
  // must not hydrate its placeholders or enqueue thumbnails after this reset.
  currentContentRequestId++;
  invalidateThumbnailRequests();
  scanStreamAlbumId.value = albumId;
  void syncScanStreamingTitle(albumId);
  clearSelectionForFileListUpdate();
  clearContentRows();
  totalFileCount.value = 0;
  totalFileSize.value = 0;
  selectedItemIndex.value = -1;
  isLoading.value = false;
  hasLoadedInitialResult.value = true;
  contentReady.value = true;
  currentQuerySource.value = 'query';
  currentSmartQueryParams.value = null;
  currentCollectionId.value = null;
  currentSearchFileIds.value = [];
  currentQueryParams.value = buildScanStreamQueryParams();
  timelineData.value = [];
  lastVisibleRange = { start: -1, end: -1 };
  visibleRangeSeqId++;
  markDedupSourceUpdated();
}

async function pullScanStreamingDelta(albumId: number, current: number) {
  if(scanHasPendingEdits())return;
  if (!isScanStreamingMode.value || Number(libConfig.album.id) !== Number(albumId)) {
    return;
  }

  if (scanStreamAlbumId.value !== albumId) {
    enterScanStreamingMode(albumId);
  }

  const targetCount = Math.max(0, Number(current || 0));
  const offset = fileList.value.length;
  const gap = targetCount - offset;
  if (gap <= 0) return;

  for (let i = 0; i < gap; i++) {
    const idx = offset + i;
    fileList.value.push({
      id: `scan-ph-${albumId}-${idx}`,
      isPlaceholder: true,
      name: '',
      size: 0,
      isSelected: false,
      rotate: 0,
    });
  }

  totalFileCount.value = fileList.value.length;
  const startIndex = lastVisibleRange.start >= 0 ? lastVisibleRange.start : 0;
  const endIndex =
    lastVisibleRange.end > startIndex
      ? lastVisibleRange.end
      : Math.min(fileList.value.length, selectionChunkSize.value);
  queueScanVisiblePrefetch(startIndex, endIndex);
  markDedupSourceUpdated();
}

function queueScanVisiblePrefetch(startIndex: number, endIndex: number) {
  const buffer = 200;
  scanVisiblePrefetchStart.value = startIndex - buffer;
  scanVisiblePrefetchEnd.value = endIndex + buffer;

  if (scanVisiblePrefetchTimer) return;
  scanVisiblePrefetchTimer = setTimeout(() => {
    scanVisiblePrefetchTimer = null;
    fetchDataRange(scanVisiblePrefetchStart.value, scanVisiblePrefetchEnd.value);
  }, 80);
}

async function scheduleScanStreamingPull(albumId: number, current: number) {
  if (scanStreamRequestInFlight.value) {
    scanStreamPullPending.value = true;
    return;
  }

  scanStreamRequestInFlight.value = true;
  try {
    await pullScanStreamingDelta(albumId, current);
  } finally {
    scanStreamRequestInFlight.value = false;
    if (scanStreamPullPending.value) {
      scanStreamPullPending.value = false;
      await scheduleScanStreamingPull(albumId, libConfig.index.discovered);
    }
  }
}

function queueScanStreamingPull(albumId: number, current: number) {
  scanStreamQueuedAlbumId.value = albumId;
  scanStreamQueuedIndexed.value = Math.max(scanStreamQueuedIndexed.value, Number(current || 0));

  if (scanStreamFlushTimer) return;
  scanStreamFlushTimer = setTimeout(async () => {
    scanStreamFlushTimer = null;
    const queuedAlbumId = scanStreamQueuedAlbumId.value;
    const queuedIndexed = scanStreamQueuedIndexed.value;
    scanStreamQueuedAlbumId.value = null;
    scanStreamQueuedIndexed.value = 0;

    if (!queuedAlbumId || queuedIndexed < 0) return;
    await scheduleScanStreamingPull(queuedAlbumId, queuedIndexed);

    const currentAlbumId = Number(libConfig.album.id || 0);
    if (
      isScanStreamingMode.value &&
      currentAlbumId > 0 &&
      Number(libConfig.index.albumQueue[0] || 0) === currentAlbumId &&
      Number(libConfig.index.discovered || 0) > fileList.value.length
    ) {
      queueScanStreamingPull(currentAlbumId, Number(libConfig.index.discovered || 0));
    }
  }, 300);
}

function restoreScrollAfterRefresh() {
  const scrollTop = pendingRestoreScrollTop.value;
  if (scrollTop === null) return;
  nextTick(() => {
    if (gridViewRef.value) {
      gridViewRef.value.scrollToPosition(scrollTop);
    }
    pendingRestoreScrollTop.value = null;
  });
}

watch(isIndexing, (val) => {
  if (!val) {
    if (suppressNextIndexingIdleRefresh.value) {
      suppressNextIndexingIdleRefresh.value = false;
      return;
    }
    if(scanHasPendingEdits()){deferScanPresentation();return;}
    updateContent();
  }
});

watch(
  () => [config.main.sidebarIndex, libConfig.activePane, libConfig.album.id, isAnyIndexing.value],
  () => {
    if (!isScanStreamingMode.value) {
      scanStreamAlbumId.value = null;
      scanStreamPullPending.value = false;
      scanStreamQueuedAlbumId.value = null;
      scanStreamQueuedIndexed.value = 0;
      if (scanStreamFlushTimer) {
        clearTimeout(scanStreamFlushTimer);
        scanStreamFlushTimer = null;
      }
      if (scanVisiblePrefetchTimer) {
        clearTimeout(scanVisiblePrefetchTimer);
        scanVisiblePrefetchTimer = null;
      }
    }
  }
);

watch(
  () => [config.main.sidebarIndex, libConfig.activePane, libConfig.album.id, activeScanningAlbumId.value],
  ([sidebarIndex, activePane, albumId, activeId]) => {
    const targetAlbumId = Number(activeId || 0);
    if (
      sidebarIndex === SIDEBAR.ALBUM &&
      activePane !== 'collection' &&
      Number(albumId || 0) > 0 &&
      Number(albumId || 0) === targetAlbumId &&
      targetAlbumId > 0
    ) {
      // Re-entering album tab should immediately sync placeholder length
      // to current indexed count, instead of waiting for next progress tick.
      if (scanStreamAlbumId.value !== targetAlbumId) {
        enterScanStreamingMode(targetAlbumId);
      }
      queueScanStreamingPull(targetAlbumId, Number(libConfig.index.discovered || 0));
    }
  }
);

watch(
  () => [libConfig.index.discovered, libConfig.album.id, config.main.sidebarIndex, libConfig.activePane, libConfig.album.selected],
  ([discovered, albumId, sidebarIndex, activePane, selected]) => {
    if (
      sidebarIndex === SIDEBAR.ALBUM &&
      activePane !== 'collection' &&
      Number(albumId) > 0 &&
      Number(libConfig.index.albumQueue[0] || 0) === Number(albumId) &&
      Number(discovered || 0) >= 0
    ) {
      queueScanStreamingPull(Number(albumId), Number(discovered || 0));
    }
  },
  { immediate: true }
);

// Cancel indexing for current album
async function cancelIndexing() {
  showIndexRecoveryMsgbox.value=false;
  try {await pauseAlbumScan(Number(libConfig.album.id));}catch(error){toast.error(t('scan_tracking.pause_failed',{error:String(error)}));}
}

onMounted( async() => {
  const appConfig = await getAppConfig();
  pendingInitialSelectedIndex = Number(appConfig?.last_selected_item_index ?? -1);
  hasRestoredInitialSelection = false;

  window.addEventListener('keydown', handleLocalKeyDown);
  window.addEventListener(PREVIEW_WINDOW_FOCUS_RESTORED, restoreOpenPreviewFocus);
  window.addEventListener('keyup', handleLocalKeyUp);
  unlistenAlbumAccessChanged = await listen('album-accessibility-changed', (event: any) => {
    if (event.payload?.libraryId !== libConfig._libraryId || event.payload?.selectionChanged) return;
    if (config.main.sidebarIndex === SIDEBAR.ALBUM && Number(libConfig.album.id) > 0
      && Number(event.payload?.albumId || libConfig.album.id) !== Number(libConfig.album.id)) return;
    pendingInitialSelectedIndex = selectedItemIndex.value;
    hasRestoredInitialSelection = false;
    pendingRestoreScrollTop.value = gridViewRef.value?.getScrollTop() ?? null;
    updateContent(true);
  });
  unlistenKeydown = await listen('global-keydown', handleKeyDown);

  unlistenLibraryTotalRefreshed = await listen('library-total-refreshed', (event: any) => {
    if (event?.payload?.source === 'content') return;
    if (isScanStreamingMode.value) return;
    if (config.main.sidebarIndex === SIDEBAR.ALBUM) {
      pendingInitialSelectedIndex = selectedItemIndex.value;
      hasRestoredInitialSelection = false;
      updateContent(true);
    }
  });
  unlistenImportFilesAdded = await listen('import-files-added', (event: any) => {
    void refreshImportedAlbumContent(Number(event.payload?.albumId || 0));
  });
  unlistenImportFilesToFolder = await listen('import-files-to-folder', (event: any) => {
    const albumId = Number(event.payload?.albumId || 0);
    const folderPath = String(event.payload?.folderPath || '');
    if (albumId > 0 && folderPath) void importFilesToFolder({ albumId, folderPath });
  });
  unlistenPasteClipboard = await listen('paste-clipboard-to-folder', (event: any) => {
    const albumId = Number(event.payload?.albumId || 0);
    const folderPath = String(event.payload?.folderPath || '');
    if (albumId > 0 && folderPath) {
      void pasteClipboardImage({ albumId, folderPath });
    }
  });

  // Drag-drop file import. Tauri native drag/drop is disabled so internal
  // HTML5 drag interactions (e.g. sortable lists) keep their drop events.
  domDragEnter = (e: DragEvent) => {
    if (isReturningNativeFileDrag(e)) {
      e.preventDefault();
      clearDropOverlay();
      return;
    }
    if (isInternalReorderActive()) {
      clearDropOverlay();
      return;
    }
    if (!hasExternalDragIntent(e)) return;
    updateExternalDrag(e);
    if (externalDragKind.value !== 'files') {
      clearDropOverlay();
      return;
    }
    e.preventDefault();
    if (acceptDrops.value) {
      dragOverCount.value++;
      isDragOver.value = true;
    }
  };
  domDragLeave = (e: DragEvent) => {
    if (isReturningNativeFileDrag(e)) {
      e.preventDefault();
      clearDropOverlay();
      return;
    }
    if (isInternalReorderActive()) {
      clearDropOverlay();
      return;
    }
    if (!hasExternalDragIntent(e)) return;
    e.preventDefault();
    dragOverCount.value = Math.max(0, dragOverCount.value - 1);
    if (dragOverCount.value === 0) isDragOver.value = false;
  };
  domDragOver = (e: DragEvent) => {
    if (isReturningNativeFileDrag(e)) {
      e.preventDefault();
      clearDropOverlay();
      return;
    }
    if (isInternalReorderActive()) {
      clearDropOverlay();
      return;
    }
    if (!hasExternalDragIntent(e)) return;
    updateExternalDrag(e);
    if (externalDragKind.value !== 'files') {
      clearDropOverlay();
      return;
    }
    if (acceptDrops.value) isDragOver.value = true;
    e.preventDefault();
  };
  domDrop = async (e: DragEvent) => {
    finishExternalDrag();
    if (isReturningNativeFileDrag(e)) {
      e.preventDefault();
      clearDropOverlay();
      return;
    }
    if (isInternalReorderActive() || isContentInternalDrag.value) {
      clearDropOverlay();
      clearContentInternalDrag();
      return;
    }
    if (!hasExternalDomDrop(e)) {
      clearDropOverlay();
      return;
    }
    e.preventDefault();
    clearDropOverlay();
    updateExternalDrag(e);
    if (externalDragKind.value === 'folders' || externalDragKind.value === 'mixed') return;
    if (!acceptDrops.value) {
      showDropWarning.value = true;
      return;
    }
    const dt = e.dataTransfer;
    if (!dt) return;
    const droppedFiles = Array.from(dt.files);
    const droppedUris = getExternalDropUris(dt);
    const nativeDragPayloadPromise = isMac
      ? getDragPayload() as Promise<{ filePaths?: string[]; url?: string | null }>
      : Promise.resolve(null);
    const destination = await resolveCurrentAlbumImportDestination();
    if (!destination) return;
    const nativeDragPayload = await nativeDragPayloadPromise;
    const { albumId, folderId, folderPath } = destination;

    let imported = 0;
    const filePaths = [
      ...getExternalFileDropPaths(droppedUris),
      ...(nativeDragPayload?.filePaths || []),
    ];
    for (const filePath of [...new Set(filePaths)]) {
      const file = await importFile(filePath, folderId, folderPath);
      if (file) imported++;
    }
    if (imported > 0) {
      await refreshImportedFiles(albumId, folderPath);
      toast.success(t('msgbox.drop_import.success', { count: imported }));
      return;
    }

    if (droppedFiles.length > 0) {
      const MAX_FILE_SIZE = 200 * 1024 * 1024; // 200 MB
      const extRE = /\.(\w+)$/i;
      imported = 0;
      for (const file of droppedFiles) {
        if (!extRE.test(file.name) || file.size <= 0 || file.size > MAX_FILE_SIZE) continue;
        try {
          const buf = await file.arrayBuffer();
          if (buf.byteLength === 0) continue;
          const bytes = Array.from(new Uint8Array(buf));
          const result = await importFileBytes(bytes, file.name, folderId, folderPath);
          if (result) imported++;
        } catch (err) {
          console.error('Failed to import file via bytes:', file.name, err);
        }
      }
      if (imported > 0) {
        await refreshImportedFiles(albumId, folderPath);
        toast.success(t('msgbox.drop_import.success', { count: imported }));
        return;
      }
      // Fall through to URL handling — some browsers provide both
      // file-like items and text/uri-list.
    }

    const urls = [
      ...getExternalHttpDropUrls(droppedUris),
      ...(nativeDragPayload?.url ? [nativeDragPayload.url] : []),
    ];
    if (urls.length > 0) {
      void handleDropUrls([...new Set(urls)], destination);
      return;
    }
    toast.warning(t('msgbox.drop_import.no_files'));
  };
  domDragEnd = () => { clearDropOverlay(); resetExternalDrag(); };
  document.addEventListener('dragenter', domDragEnter);
  document.addEventListener('dragleave', domDragLeave);
  document.addEventListener('dragover', domDragOver);
  document.addEventListener('dragend', domDragEnd);
  document.addEventListener('drop', domDrop);

  unlistenImageViewer = await listen('message-from-image-viewer', async (event) => {
    const { message } = event.payload as any;
    switch (message) {
      case 'request-file-at-index':
        const requestIndex = (event.payload as any).index;
        const requestedPane = String((event.payload as any).pane || 'left');
        const pane = ['left', 'right', 'bottomLeft', 'bottomRight'].includes(requestedPane)
          ? requestedPane
          : 'left';
        const session = imageViewerSession.value;
        if (session.mode === 'normal') {
          if (!isRealFileItem(fileList.value[requestIndex])) {
            ensureGroupedFileAtIndex(requestIndex);
          }
          if (!isRealFileItem(fileList.value[requestIndex])) {
            await fetchDataRange(requestIndex, requestIndex + 2);
          }
          // Reflect the viewer's navigation in the main-window selection so the
          // grid highlight/scroll follows the image being viewed.
          if (pane === 'left' && selectedItemIndex.value !== requestIndex) {
            suppressImageViewerSync = true;
            selectedItemIndex.value = requestIndex;
            void nextTick(() => { suppressImageViewerSync = false; });
          }
        }
        const viewerFiles = session.mode === 'compare' ? session.files : fileList.value;
        const file = viewerFiles[requestIndex];
        if (isRealFileItem(file)) {
           const imageWindow = await WebviewWindow.getByLabel('imageviewer');
           if (imageWindow) {
             imageWindow.emit('update-img', {
               fileId: file.id,
               fileIndex: requestIndex,
               fileCount: viewerFiles.length,
               nextFilePath: (() => {
                 const next = viewerFiles[requestIndex + 1];
                 return next && !next.isPlaceholder && next.file_type === 1 ? next.file_path : '';
               })(),
               pane,
             });
           }
        }
        break;
      case 'navigate-info':
        await handleViewerInfoNavigation(event.payload as any);
        break;
      case 'open-chat-media':
        try { await handleChatMediaNavigation(event.payload as any); }
        catch (error) { toast.error(String(error)); }
        break;
      case 'update-file-meta':
        const targetFileId = Number((event.payload as any).fileId);
        const changes = (event.payload as any).changes || {};
        if (targetFileId > 0) {
          const index = fileList.value.findIndex(file => file.id === targetFileId);
          if (index !== -1) {
            Object.assign(fileList.value[index], changes);
          }
          if (changes.culling_flag !== undefined || changes.cullingFlag !== undefined) {
            void tauriEmit('culling-status-updated', { fileIds: [targetFileId], cullingFlag: changes.culling_flag ?? changes.cullingFlag });
          }
        }
        break;
      default:
        break;
    }
  });

  unlistenMontage = await listen('message-from-montage', async (event: any) => {
    const { type, filePath } = event.payload as any;
    if (type !== 'success') return;
    try {
      await (await WebviewWindow.getByLabel('montage'))?.destroy();
    } catch (error) {
      console.error('Failed to destroy Montage window from parent:', error);
    }
    // ponytail: saved elsewhere, the file is not indexed now; it appears when its folder is scanned
    if (normalizePathForCompare(getFolderPath(filePath)) === normalizePathForCompare(montageSourceFolder)) {
      await onFileSaved(true, { saveAsNew: true, filePath, saveAsContext: montageSaveAsContext });
    } else {
      toast.success(localeMsg.value.tooltip.save_image.save_as_success || localeMsg.value.tooltip.save_image.success);
    }
  });

  unlistenImageEditor = await listen('message-from-image-editor', async (event: any) => {
    const { type, saveAsNew, filePath, sourceFileId } = event.payload as any;
    const sourceId = Number(sourceFileId || 0);
    if (type === 'success') {
      const saveAsContext = sourceId > 0 ? imageEditorSaveAsContexts.get(sourceId) || null : null;
      if (sourceId > 0) imageEditorSaveAsContexts.delete(sourceId);
      try {
        const editorWindow = await WebviewWindow.getByLabel('imageeditor');
        if (editorWindow) {
          try {
            await editorWindow.destroy();
          } catch (error) {
            console.error('Failed to destroy ImageEditor window from parent:', error);
          }
        }

        if (!saveAsNew && filePath) {
          uiStore.updateFileVersion(filePath);
        }
        await onFileSaved(true, { saveAsNew, filePath, saveAsContext });
      } catch (error) {
        console.error('Failed handling ImageEditor save success:', error);
      }
    } else if (type === 'failed') {
      if (sourceId > 0) imageEditorSaveAsContexts.delete(sourceId);
      await onFileSaved(false);
    }
  });

  // Indexing listeners
  unlistenIndexProgress = await listenIndexProgress(async (event: any) => {
    if(event.payload?.run_id && activeScanRunId.value && event.payload.run_id!==activeScanRunId.value)return;
    const { album_id, phase, current, discovered, processed, search_ready, total, search_total, current_size, failed } = event.payload;
    if (libConfig.index.albumQueue.length > 0 && libConfig.index.albumQueue[0] === album_id) {
        libConfig.index.phase = String(phase || libConfig.index.phase || 'discovering');
        libConfig.index.discovered = Number(discovered || 0);
        libConfig.index.processed = Number(processed || current || 0);
        libConfig.index.searchReady = Number(search_ready || 0);
        libConfig.index.indexed = Number(processed || current || 0);
        libConfig.index.total = Number(total || 0);
        libConfig.index.searchTotal = Number(search_total || 0);
        libConfig.index.failed = Number(failed || 0);
        if (
          config.main.sidebarIndex === SIDEBAR.ALBUM &&
          Number(libConfig.album.id || 0) === Number(album_id || 0)
        ) {
          totalFileSize.value = Number(current_size || 0);
        }
    }
  });

  unlistenIndexFinished = await listenIndexFinished(async (event: any) => {
    if(event.payload?.run_id && activeScanRunId.value && event.payload.run_id!==activeScanRunId.value)return;
    const { album_id, phase, indexed, processed, search_ready, total, search_total, failed } = event.payload;
    // notify album list to update cover

    const shouldRefreshCurrentView =
      config.main.sidebarIndex === SIDEBAR.ALBUM &&
      Number(libConfig.album.id) > 0 &&
      Number(libConfig.album.id) === Number(album_id);

    if(shouldRefreshCurrentView)suppressNextIndexingIdleRefresh.value=true;
    if (libConfig.index.albumQueue.length > 0 && libConfig.index.albumQueue[0] === album_id) {
      libConfig.index.phase = String(phase || 'complete');
      libConfig.index.discovered = Number(total || 0);
      libConfig.index.processed = Number(processed || indexed || 0);
      libConfig.index.searchReady = Number(search_ready || 0);
      libConfig.index.indexed = Number(processed || indexed || 0);
      libConfig.index.total = Number(total || 0);
      libConfig.index.searchTotal = Number(search_total || 0);
      libConfig.index.failed = Number(failed || 0);
      libConfig.index.albumQueue.shift();activeScanRunId.value='';
      if(event.payload.status==='paused'||event.payload.status==='interrupted'){
        if(!isAlbumPaused(album_id))libConfig.index.pausedAlbumIds.push(Number(album_id));
      }else libConfig.index.pausedAlbumIds=libConfig.index.pausedAlbumIds.filter((id:any)=>Number(id)!==Number(album_id));
      if (libConfig.index.albumQueue.length > 0) {
        processNextAlbum();
      } else {
        syncIndexStatus();
      }
    }

    await tauriEmit('album-cover-changed', { albumId: album_id, fileId: null, libraryId: event.payload.library_id });

    if (shouldRefreshCurrentView) {
      scanStreamAlbumId.value = null;
      scanStreamPullPending.value = false;
      scanStreamQueuedAlbumId.value = null;
      scanStreamQueuedIndexed.value = 0;
      if (scanStreamFlushTimer) {
        clearTimeout(scanStreamFlushTimer);
        scanStreamFlushTimer = null;
      }
      if (scanVisiblePrefetchTimer) {
        clearTimeout(scanVisiblePrefetchTimer);
        scanVisiblePrefetchTimer = null;
      }
      if (gridViewRef.value) {
        pendingRestoreScrollTop.value = gridViewRef.value.getScrollTop();
      }
      // Avoid duplicated refresh: this explicit refresh replaces the
      // watch(isIndexing) idle refresh for this finish cycle.
      deferScanPresentation(Number(album_id));
      setTimeout(flushScanPresentation,200);
    }

    refreshLibraryTotalCount();
  });

  unlistenThumbnailReady = await listen('thumbnail_ready', (event: any) => {
    void handleThumbnailReady(event).catch(error => {
      console.warn('Failed to update ready thumbnails:', error);
    });
  });

  // listen for external refresh requests (e.g. from folder context menu)
  const stopMedia=await listen('media-lifecycle-changed',(event:any)=>{
    if(event.payload?.library_id!==libConfig._libraryId)return;
    libConfig.clearLazySidebarCounts();
    const blocked=new Set((event.payload?.file_ids||[]).map(Number));
    libConfig.smartAlbums=(libConfig.smartAlbums||[]).map((a:any)=>blocked.has(Number(a.coverFileId))?{...a,coverFileId:null,count:null}:a);
    if(config.main.sidebarIndex===SIDEBAR.LIBRARY && [LIB_ITEM.HIDDEN,LIB_ITEM.DELETED].includes(libConfig.library.item))return;
    rememberFocusedFileForPresentationRefresh();void updateContent(true);
  });smartEventStops.push(stopMedia);
  unlistenRefreshContent = await listen('refresh-content', () => {
    // A manual folder refresh must bypass the mtime-sync debounce; otherwise
    // revisiting an already selected folder only reloads stale DB rows.
    lastSyncedAlbumFolder = '';
    updateContent();
  });

  scanHistoryStops.push(await listen<ScanScope>('scan-history-requested',event=>{
    if(event.payload?.libraryId===libConfig._libraryId&&Number(event.payload.albumId)>0)scanHistoryScope.value=event.payload;
  }));
  scanHistoryStops.push(await listen<any>('scan-retry-finished',event=>{
    if(event.payload?.library_id!==libConfig._libraryId)return;
    void refreshLibraryTotalCount();
    if(config.main.sidebarIndex===SIDEBAR.ALBUM && Number(libConfig.album.id)===Number(event.payload.album_id)){
      deferScanPresentation(Number(event.payload.album_id));flushScanPresentation();
    }
  }));
  scanHistoryStops.push(await listen<any>('scan-album-requested',event=>{
    if(event.payload?.libraryId!==libConfig._libraryId)return;
    const id=Number(event.payload.albumId);if(!(id>0)||scanJobs.pausing.has(id)||albumScanRun(id)?.retry?.running)return;
    libConfig.index.pausedAlbumIds=libConfig.index.pausedAlbumIds.filter((value:any)=>Number(value)!==id);
    if(!libConfig.index.albumQueue.some((value:any)=>Number(value)===id))libConfig.index.albumQueue.push(id);
    libConfig.index.status=1;void processNextAlbum();
  }));
  unlistenTriggerNextAlbum = await listen('trigger-next-album', () => {
    processNextAlbum();
  });

  unlistenFilesDeleted = await listen('files-deleted', async (event: any) => {
    if (event?.payload?.source === 'content') return;
    const deletedIds = Array.isArray(event?.payload?.fileIds)
      ? event.payload.fileIds.map((id: any) => Number(id)).filter((id: number) => id > 0)
      : [];
    const compareFileCount = removeDeletedFilesFromImageViewerSession(deletedIds);
    if (compareFileCount !== undefined) {
      // Re-broadcast with the comparison count after its session snapshot has
      // been updated. The ImageViewer must never fall back to the full list.
      await tauriEmit('files-deleted', {
        source: 'content',
        fileIds: deletedIds,
        fileCount: fileList.value.length,
        compareFileCount,
      });
    }
    if (deletedIds.length === 0 || fileList.value.length === 0) return;

    if (tempViewMode.value !== 'none') {
      updateContent();
      return;
    }

    const deleteSet = new Set(deletedIds);
    let removedAny = false;
    for (let i = fileList.value.length - 1; i >= 0; i--) {
      if (deleteSet.has(fileList.value[i].id)) {
        fileList.value.splice(i, 1);
        removedAny = true;
      }
    }
    if (!removedAny) return;

    totalFileCount.value = fileList.value.length;
    totalFileSize.value = fileList.value.reduce((total, file) => total + file.size, 0);
    if (fileList.value.length === 0) {
      selectedItemIndex.value = -1;
    } else {
      selectedItemIndex.value = Math.min(selectedItemIndex.value, fileList.value.length - 1);
      if (selectedItemIndex.value < 0) selectedItemIndex.value = 0;
    }
    await updateSelectedImage(selectedItemIndex.value);
  });

  unlistenAlbumUpdated = await listen('album-updated', (event: any) => {
    const { albumId, name, filtersChanged } = event.payload || {};
    if (filtersChanged) {
      clearFolderFileCounts();
      uiStore.clearCountUpdateRequest();
      libConfig.clearLazySidebarCounts();
      scheduleContentRefresh(() => refreshContentFromSelectionChange());
    }
    const targetId = Number(albumId || 0);
    if (targetId <= 0 || !name) return;
    for (const file of fileList.value) {
      if (Number(file.album_id) === targetId) {
        file.album_name = name;
      }
    }
  });

  // Reconcile a persisted head only when its exact job is known to have ended.
  await refreshScanJobs();
  const savedScan=scanJobs.runs[activeScanRunId.value];
  if(savedScan?.finished_at && Number(libConfig.index.albumQueue[0])===savedScan.album_id){
    const wasPaused=Number(libConfig.index.status)===2;
    libConfig.index.albumQueue.shift();activeScanRunId.value='';
    libConfig.index.pausedAlbumIds=libConfig.index.pausedAlbumIds.filter((id:any)=>Number(id)!==savedScan.album_id);
    if(savedScan.status==='paused')libConfig.index.pausedAlbumIds.push(savedScan.album_id);
    if(wasPaused&&libConfig.index.albumQueue.length)libConfig.index.status=2;else syncIndexStatus();
  }
  if (libConfig.index.albumQueue.length > 0 && libConfig.index.status === 1) {
    processNextAlbum();
  }

  // Face Indexing listeners
  unlistenFaceIndexProgress = await listenFaceIndexProgress((event: any) => {
    if (config.main.sidebarIndex === SIDEBAR.PERSON && fileList.value.length > 0) {
      clearContentRows();
      totalFileCount.value = 0;
      totalFileSize.value = 0;
      scrollPosition.value = 0;
      selectedItemIndex.value = -1;
    }
  });
});

function restoreInitialSelectionIfNeeded() {
  if (hasRestoredInitialSelection || pendingInitialSelectedIndex < 0 || fileList.value.length === 0) {
    return;
  }

  selectedItemIndex.value = Math.min(pendingInitialSelectedIndex, fileList.value.length - 1);
  hasRestoredInitialSelection = true;
  void updateSelectedImage(selectedItemIndex.value);
}

onBeforeUnmount(() => {
  if (scanStreamFlushTimer) {
    clearTimeout(scanStreamFlushTimer);
    scanStreamFlushTimer = null;
  }
  if (scanVisiblePrefetchTimer) {
    clearTimeout(scanVisiblePrefetchTimer);
    scanVisiblePrefetchTimer = null;
  }
  if (layoutRefreshTimer) {
    clearTimeout(layoutRefreshTimer);
    layoutRefreshTimer = null;
  }
  unlistenAlbumAccessChanged?.();
  window.removeEventListener('keydown', handleLocalKeyDown);
  window.removeEventListener(PREVIEW_WINDOW_FOCUS_RESTORED, restoreOpenPreviewFocus);
  window.removeEventListener('keyup', handleLocalKeyUp);
  // unlisten
  unlistenImageViewer();
  if (unlistenImageEditor) unlistenImageEditor();
  if (unlistenMontage) unlistenMontage();
  if (unlistenKeydown) unlistenKeydown();
  if (unlistenTriggerNextAlbum) unlistenTriggerNextAlbum();
  scanHistoryStops.forEach(stop=>stop());
  if (unlistenIndexProgress) unlistenIndexProgress();
  if (unlistenIndexFinished) unlistenIndexFinished();
  if (unlistenThumbnailReady) unlistenThumbnailReady();
  if (unlistenRefreshContent) unlistenRefreshContent();
  if (unlistenFilesDeleted) unlistenFilesDeleted();
  if (unlistenAlbumUpdated) unlistenAlbumUpdated();
  if (unlistenFaceIndexProgress) unlistenFaceIndexProgress();
  if (unlistenImportFilesToFolder) unlistenImportFilesToFolder();
  if (unlistenPasteClipboard) unlistenPasteClipboard();
  if (domDragEnter) document.removeEventListener('dragenter', domDragEnter);
  if (domDragLeave) document.removeEventListener('dragleave', domDragLeave);
  if (domDragOver) document.removeEventListener('dragover', domDragOver);
  if (domDragEnd) document.removeEventListener('dragend', domDragEnd);
  if (domDrop) document.removeEventListener('drop', domDrop);
  removeDragGhost();
});

/// watch appearance
watch(
  () => [config.settings.rawPreviewSource, config.settings.rawRenderBrightness, config.settings.rawPairDisplaySource, config.settings.groupRawJpegPairs],
  () => {
    invalidateThumbnailRequests();
    for (const file of fileList.value) {
      if (Number(file.file_type) === 3) {
        clearCachedThumbnailDataUrl(file.id, config.settings.thumbnailSize);
        file.rawThumbnailStale = true;
      }
    }
    const start = Math.max(0, lastVisibleRange.start);
    void fetchMissingVisibleThumbnails(start, Math.max(start + 40, lastVisibleRange.end));
  },
);

watch(() => config.settings.appearance, (newAppearance) => {
  setTheme(newAppearance, newAppearance === 0 ? config.settings.lightTheme : config.settings.darkTheme);
});

/// watch light theme
watch(() => config.settings.lightTheme, (newLightTheme) => {
  setTheme(config.settings.appearance, newLightTheme);
});

/// watch dark theme
watch(() => config.settings.darkTheme, (newDarkTheme) => {
  setTheme(config.settings.appearance, newDarkTheme);
});

/// watch language
watch(() => config.settings.language, (newLanguage) => {
    locale.value = newLanguage; // update locale based on config.settings.language
    updateContent();
});

// Load tags when info panel is opened
watch(() => isInfoPanelOpen.value, async (newShow) => {
  const index = selectedItemIndex.value;
  if (newShow && index >= 0 && index < fileList.value.length) {
    const file = fileList.value[index];
    if (isRealFileItem(file) && file.has_tags && !file.tags) {
      // Load tags if has_tags is true but tags not yet loaded
      const tags = await getTagsForFile(file.id);
      if (fileList.value[index] === file) {
        file.tags = tags;
      }
    }
  }
});

watch(() => libConfig.index.status, (newStatus) => {
  if (newStatus === 1 && libConfig.index.albumQueue.length > 0 && !isRecoveryInProgress.value) {
    processNextAlbum();
  }
});

watch(() => libConfig.index.albumQueue.length, (newLength) => {
   if (newLength > 0 && libConfig.index.status === 0) {
       libConfig.index.status = 1; 
   }
});

watch(
  () => [config.settings.showSubfolderFiles, libConfig._libraryId],
  () => {
    clearFolderFileCounts();
  },
);

watch(() => libConfig._libraryId, () => {
  uiStore.clearCountUpdateRequest();
});

/// watch for file list changes
watch(
  () => JSON.stringify({
    sidebar: config.main.sidebarIndex,
    pane: libConfig.activePane,
    collectionId: libConfig.collection.selectedId,
    libraryItem: libConfig.library.item,
    smartId: libConfig.library.smartId,
    album: [libConfig.album.id, libConfig.album.folderId, libConfig.album.folderPath, libConfig.album.selected],
    smartAlbum: [libConfig.smartAlbum.type, libConfig.smartAlbum.id],
    objects: config.main.sidebarIndex === SIDEBAR.OBJECTS ? [libConfig.objects.state, libConfig.objects.classKey, libConfig.objects.minCount, config.settings.ai?.detectionProfile] : null,
    ocr: config.main.sidebarIndex === SIDEBAR.OCR ? [libConfig.ocr.state, libConfig.ocr.text, config.settings.ai?.ocrProfile] : null,
    personId: libConfig.person.id,
    calendar: [config.calendar.view, libConfig.calendar.year, libConfig.calendar.month, libConfig.calendar.date],
    tagId: libConfig.tag.id,
    tagGroupId: libConfig.tag.groupId,
    location: libConfig.location.nodeId,
    camera: [config.camera.isCamera, libConfig.camera.make, libConfig.camera.model, (libConfig.camera as any).lensMake, (libConfig.camera as any).lensModel],
  }),
  (nextView, previousView) => {
    if (previousView !== undefined && nextView !== previousView) {
      pendingFocusedFileId = null;
      if (selectMode.value) selectNoneInCurrentList();
    }
  },
);

watch(
  () => [
    config.main.sidebarIndex,      // toolbar index
    libConfig.activePane,          // main content or collection tray
    libConfig.collection.selectedId, // collection
    (libConfig.library as any).item, // library
    libConfig.library.smartId,
    libConfig.album.id, libConfig.album.folderId, libConfig.album.folderPath, libConfig.album.selected, // album
    libConfig.smartAlbum.type, libConfig.smartAlbum.id,
    // Count and cover updates are query outputs, not refresh triggers.
    smartResultSignature(activeSmartAlbum.value), // Only active result-affecting changes reload photos; folders, names, and counters do not.
    isStructuredSearchView.value,
    libConfig.search.searchText,
    config.main.sidebarIndex === SIDEBAR.SEARCH ? JSON.stringify([mediaAiSearch.active, mediaAiSearch.libraryId, mediaAiSearch.source, mediaAiSearch.modelId, mediaAiSearch.albumId, mediaAiSearch.revision]) : null,
    config.main.sidebarIndex === SIDEBAR.OBJECTS ? JSON.stringify([libConfig.objects, config.settings.ai?.detectionProfile]) : null,
    config.main.sidebarIndex === SIDEBAR.OCR ? `${libConfig.ocr.state}\0${libConfig.ocr.text}` : null,
    [SIDEBAR.OCR, SIDEBAR.SEARCH].includes(config.main.sidebarIndex) ? config.settings.ai?.ocrProfile : null,
    libConfig.rating.item, // rating
    libConfig.culling.item, // culling
    config.search.fileType, config.search.sortType, config.search.sortOrder, // search and sort 
    config.settings.showSubfolderFiles,                                            // album folder view
    config.settings.folderSort, config.settings.calendarSort, config.settings.locationSort, config.settings.cameraSort, config.search.groupBy, // group sorting and filtering (tag/person sorts only affect their sidebars, not content grouping)
    libConfig.person.id,                                                              // person
    config.calendar.view, libConfig.calendar.year, libConfig.calendar.month, libConfig.calendar.date, // calendar
    libConfig.tag.id, libConfig.tag.groupId, libConfig.tag.activateTick, // tag
    libConfig.location.nodeId,                               // location
    libConfig.camera.make, libConfig.camera.model,                                    // camera 
    config.camera.isCamera, (libConfig.camera as any).lensMake, (libConfig.camera as any).lensModel, // lens
  ], 
  (values, previousValues) => {
    // Replacing smartAlbums creates a new outer array even if these values match.
    if (previousValues && values.every((value, index) => value === previousValues[index])) return;
    contentQueryRefreshPending = true;
    // Clear active adjustments when the file list changes to avoid unnecessary confirmation dialogs
    uiStore.clearActiveAdjustments();

    // Entering the temporary person result updates person.id so face overlays
    // can identify the matched person. Ignore only that internal sync; later
    // query-context changes exit this temporary view like similar-image search.
    if (tempViewMode.value === 'person' && suppressPersonContextRefresh) {
      contentQueryRefreshPending = false;
      return;
    }

    // A changed query context invalidates other temporary result lists. Return
    // to the normal view instead of refreshing or retaining the temporary list.
    if (tempViewMode.value !== 'none') {
      updateContent();
      return;
    }
    
    scheduleContentRefresh(() => {
      // Double check in case tempViewMode changed during setTimeout
      if (tempViewMode.value !== 'none') {
        contentQueryRefreshPending = false;
        return;
      }

      refreshContentFromSelectionChange();
    });
  }, 
  { immediate: true }
);

watch(
  () => uiStore.countUpdateTick,
  async () => {
    const request = uiStore.countUpdateRequest;
    if (
      !request
      || contentQueryRefreshPending
      || !contentReady.value
      || !contentCountIsAuthoritative.value
      || tempViewMode.value !== 'none'
    ) return;

    const requestId = ++sidebarCountRequestId;
    try {
      const result = await getCurrentQueryCountAndSum();
      if (requestId !== sidebarCountRequestId || uiStore.countUpdateRequest !== request) return;
      if (result) {
        commitRequestedSidebarCount(Number(result[0] || 0), request);
      } else {
        uiStore.clearCountUpdateRequest();
      }
    } catch (error) {
      console.error('Failed to refresh sidebar count:', error);
      if (requestId === sidebarCountRequestId && uiStore.countUpdateRequest === request) {
        uiStore.clearCountUpdateRequest();
      }
    }
  },
);

watch(
  () => Number(libConfig.album.activateTick || 0),
  () => {
    if (!showQuickView.value) return;
    showQuickView.value = false;
    stopSlideShow();
  }
);

// watch for selected item (not in select mode)
// Set while applying an index change that originated from the image viewer, so
// the push below does not echo the same index back to the viewer.
let suppressImageViewerSync = false;
watch(() => selectedItemIndex.value, (newIndex, oldIndex) => {
  if(oldIndex >= 0 && oldIndex !== newIndex && fileList.value[oldIndex]?.rotate >= 360) {
    fileList.value[oldIndex].rotate %= 360;
  }
  void setLastSelectedItemIndex(Number(newIndex ?? -1));
  updateSelectedImage(newIndex);
  // Keep an open image viewer in sync with the main-window selection.
  if (!suppressImageViewerSync) {
    void syncSelectionToImageViewer(newIndex);
  }
});

// watch for show preview or layout change
watch(() => config.settings.grid.style, () => {
  updateSelectedImage(selectedItemIndex.value);
  stopSlideShow();
});

watch(
  () => Boolean(isFilmstripView.value),
  (isFilmstrip) => {
    if (!isFilmstrip) stopSlideShow();
    // Filmstrip uses flat file order; grid needs the grouped query rows again.
    // Reload both directions so placeholders and the focused file use the
    // correct ordering, including when switching during an in-flight load.
    if (
      effectiveGroupBy.value > GROUP.NONE &&
      !isScanStreamingMode.value &&
      tempViewMode.value === 'none' &&
      currentQuerySource.value !== 'search'
    ) {
      refreshContentFromSelectionChange();
      return;
    }
    resetGroupingState();
    void nextTick(() => gridViewRef.value?.refreshLayout?.());
  },
);

function disablePreviewModes() {
  showQuickView.value = false;
  stopSlideShow();
}

watch(gridSize, (newSize, oldSize) => {
  if (newSize === oldSize) return;
  if (showQuickView.value || isSlideShow.value) {
    disablePreviewModes();
  }
});

function toggleFilmstripView() {
  showQuickView.value = false;
  // Filmstrip is a select-only view: clicking it when already active is a no-op.
  if (isFilmstripView.value) return;
  config.settings.grid.viewMode = 'filmstrip';
  filmStripZoomFit.value = true;
}

function toggleMapView() {
  if (showQuickView.value) {
    closeQuickPreview();
    return;
  }
  if (tempViewMode.value === 'map') {
    exitTempViewMode();
    return;
  }
  // Map is a select-only view: clicking it when already active is a no-op.
  if (isMapView.value) return;
  if (selectMode.value) handleSelectMode(false);
  config.settings.grid.viewMode = 'map';
}

function openMapClusterTempView(payload: { fileIds?: number[]; minLat: number; maxLat: number; minLon: number; maxLon: number; count: number; view: { lat: number; lon: number; zoom: number } }) {
  const selectionRequestId = ++mapSelectionRequestId;
  checkUnsavedChanges(async () => {
    if (payload.fileIds?.length) {
      const index = await resolveFileIndexForDedup(payload.fileIds[0]);
      if (selectionRequestId !== mapSelectionRequestId || !isMapContentVisible.value) return;
      if (index >= 0) selectedItemIndex.value = index;
    }
    enterMapClusterTempView(payload);
  });
}

function enterMapClusterTempView(payload: { fileIds?: number[]; minLat: number; maxLat: number; minLon: number; maxLon: number; count: number; view: { lat: number; lon: number; zoom: number } }) {
  if (tempViewMode.value === 'none') backupState.value = createViewBackup();
  mapTempViewState.value = payload.view;
  invalidateThumbnailRequests();
  tempViewMode.value = 'map';
  showQuickView.value = false;
  contentTitle.value = t('map.photos_in_view', { count: payload.count.toLocaleString() });
  const requestId = ++currentContentRequestId;
  showLoadingContent(requestId);
  scrollPosition.value = 0;
  selectedItemIndex.value = 0;
  const gpsParams = {
    ...currentQueryParams.value,
    gpsMinLat: payload.minLat,
    gpsMaxLat: payload.maxLat,
    gpsMinLon: payload.minLon,
    gpsMaxLon: payload.maxLon,
  };

  if (payload.fileIds) {
    void getMapSearchClusterFileList(payload.fileIds, gpsParams, requestId, true);
    return;
  }

  if (currentQuerySource.value === 'search') {
    void getMapSearchClusterFileList(currentSearchFileIds.value, gpsParams, requestId);
    return;
  }

  if (currentQuerySource.value === 'smart' && currentSmartQueryParams.value) {
    void getFileList(gpsParams, requestId, {
      source: 'smart',
      smartParams: {
        ...currentSmartQueryParams.value,
        gpsMinLat: payload.minLat,
        gpsMaxLat: payload.maxLat,
        gpsMinLon: payload.minLon,
        gpsMaxLon: payload.maxLon,
      },
    });
    return;
  }

  void getFileList(gpsParams, requestId, currentQuerySource.value === 'collection'
    ? { source: 'collection', collectionId: currentCollectionId.value }
    : null);
}

let mapPreviewRequestId = 0;
let mapSelectionRequestId = 0;
async function selectMapFile(fileId: number) {
  const requestId = ++mapSelectionRequestId;
  const index = await resolveFileIndexForDedup(fileId);
  if (requestId !== mapSelectionRequestId || index < 0) return;
  // Map selections are independent of the grid viewport used by thumbnail scheduling.
  void getFileListThumb([fileList.value[index]], { viewportBound: false });
  handleItemClicked(index);
}

async function openMapPreviewFile(fileId: number) {
  const requestId = ++mapPreviewRequestId;
  const index = await resolveFileIndexForDedup(fileId);
  if (requestId !== mapPreviewRequestId || index < 0) return;
  void getFileListThumb([fileList.value[index]], { viewportBound: false });
  handleItemDblClicked(index);
}

function cycleGridStyle() {
  showQuickView.value = false;
  // When grid layout isn't the active view (map or filmstrip), just switch to
  // grid layout; only cycle the style when grid layout is already active.
  if (config.settings.grid.viewMode !== 'grid') {
    config.settings.grid.viewMode = 'grid';
    return;
  }
  // Cycle between card, tile, justified, and masonry.
  config.settings.grid.style = (config.settings.grid.style + 1) % 4;
  void tauriEmit('settings-gridStyle-changed', config.settings.grid.style);
}

// Track pending requests to avoid duplicates
const pendingRequests = new Set();

const getCurrentQueryCountAndSum = () => {
  if (currentQuerySource.value === 'search') {
    return Promise.resolve([
      currentSearchFileIds.value.length,
      fileList.value.reduce((total, file) => total + Number(file?.size || 0), 0),
    ]);
  }
  if (currentQuerySource.value === 'collection' && currentCollectionId.value) {
    return getCollectionCountAndSum(currentCollectionId.value, currentQueryParams.value);
  }
  if (currentQuerySource.value === 'smart' && currentSmartQueryParams.value) {
    return getSmartQueryCountAndSum(currentSmartQueryParams.value);
  }
  return getQueryCountAndSum(currentQueryParams.value);
};

const getCurrentQueryTimeLine = () => {
  if (currentQuerySource.value === 'collection' || currentQuerySource.value === 'search') {
    return Promise.resolve([]);
  }
  if (currentQuerySource.value === 'smart' && currentSmartQueryParams.value) {
    return getSmartQueryTimeLine(currentSmartQueryParams.value);
  }
  return getQueryTimeLine(currentQueryParams.value);
};

const getCurrentQueryFilesBase = (offset: number, limit: number) => {
  if (currentQuerySource.value === 'search') {
    return Promise.resolve(fileList.value.slice(offset, offset + limit));
  }
  if (currentQuerySource.value === 'collection' && currentCollectionId.value) {
    return getCollectionFiles(currentCollectionId.value, currentQueryParams.value, offset, limit);
  }
  if (currentQuerySource.value === 'smart' && currentSmartQueryParams.value) {
    return getSmartQueryFiles(currentSmartQueryParams.value, offset, limit);
  }
  return getQueryFiles(currentQueryParams.value, offset, limit);
};

const getCurrentGroupedQueryRowsBase = (offset: number, limit: number) => {
  if (currentQuerySource.value === 'collection' && currentCollectionId.value) {
    return getCollectionGroupedQueryRows(currentCollectionId.value, getGroupingQueryParams(), offset, limit);
  }
  if (currentQuerySource.value === 'smart' && currentSmartQueryParams.value) {
    return getSmartGroupedQueryRows(getGroupingQueryParams(), offset, limit);
  }
  return getGroupedQueryRows(getGroupingQueryParams(), offset, limit);
};

async function enrichOcrPhotos(files: any[]) {
  if (isStructuredSearchView.value) return files;
  if (![SIDEBAR.OCR, SIDEBAR.SEARCH].includes(config.main.sidebarIndex) || libConfig.activePane === 'collection' || !files?.length) return files;
  const libraryId = libConfig._libraryId, profile = config.settings.ai?.ocrProfile;
  const text = config.main.sidebarIndex === SIDEBAR.OCR ? libConfig.ocr.text || '' : libConfig.search.searchText || '';
  const ids = files.map(file => Number(file?.id)).filter(id => id > 0);
  const summaries: any[] = [];
  try {
    for (let offset = 0; offset < ids.length; offset += 256) {
      summaries.push(...await invoke<any[]>('get_ocr_summaries', { libraryId, fileIds: ids.slice(offset, offset + 256), text }));
    }
  } catch (e) { console.error('OCR excerpts unavailable:', e); return files; }
  if (libraryId !== libConfig._libraryId || profile !== config.settings.ai?.ocrProfile) return files;
  const byId = new Map(summaries.map(row => [row.fileId, row]));
  return files.map(file => ({ ...file, _ocr: byId.get(Number(file.id)) }));
}
const getCurrentQueryFiles = async (offset: number, limit: number) => enrichOcrPhotos(await getCurrentQueryFilesBase(offset, limit) || []);
const getCurrentGroupedQueryRows = async (offset: number, limit: number) => {
  const result: any = await getCurrentGroupedQueryRowsBase(offset, limit);
  if (config.main.sidebarIndex !== SIDEBAR.OCR || !result) return result;
  const normalized = normalizeGroupedRowsResult(result);
  const files = normalized.rows.map((row: any) => row.file).filter(Boolean);
  const enriched = new Map((await enrichOcrPhotos(files)).map((file: any) => [Number(file.id), file]));
  return { ...result, rows: normalized.rows.map((row: any) => row.file ? { ...row, file: enriched.get(Number(row.file.id)) || row.file } : row) };
};

// Reconcile fetched viewport rows atomically. Do not clear the grid / close the
// viewer for every completed image; stable IDs retain thumbnails and selection.
let ocrRefreshTimer: ReturnType<typeof setTimeout> | undefined;
let ocrRefreshRunning = false, ocrRefreshAgain = false, ocrContentDisposed = false;
let offOcrContent: (() => void) | undefined, offOcrAnnotations: (() => void) | undefined;
function scheduleOcrContentRefresh() {
  ocrRefreshAgain = true;
  if (ocrRefreshTimer || ocrRefreshRunning) return;
  ocrRefreshTimer = setTimeout(() => { ocrRefreshTimer = undefined; void refreshOcrContent(); }, 900);
}
async function refreshOcrContent() {
  if (isStructuredSearchView.value) { queueSmartRefresh(); return; }
  if (ocrContentDisposed || ![SIDEBAR.OCR, SIDEBAR.SEARCH, SIDEBAR.OBJECTS].includes(config.main.sidebarIndex) || libConfig.activePane === 'collection' || showQuickView.value) return;
  if (ocrRefreshRunning) return;
  ocrRefreshRunning = true; ocrRefreshAgain = false;
  const requestId = currentContentRequestId;
  try {
    if (config.main.sidebarIndex === SIDEBAR.SEARCH) {
      rememberFocusedFileForPresentationRefresh(); captureSelectionForFileListRefresh(); currentThumbRequestId++;
      await getUnifiedSearchFileList(libConfig.search.searchText, ++currentContentRequestId);
      return;
    }
    if (groupedModeActive.value) {
      await refreshGroupedOcrContent(requestId);
      return;
    }
    const result = await getCurrentQueryCountAndSum();
    if (!result || requestId !== currentContentRequestId) return;
    const focus = fileList.value[selectedItemIndex.value];
    const position = focus?.id > 0 ? (await getQueryFilePosition(currentQueryParams.value, focus.id)) ?? -1 : -1;
    if (requestId !== currentContentRequestId) return;
    if (focus?.id > 0 && position < 0 && showFilmstripLayout.value) { ocrPreviewDeferred.value = true; return; }
    ocrPreviewDeferred.value = false;
    const count = Number(result[0]);
    let start = Math.max(0, Math.min(Math.max(0, count - 1), lastVisibleRange.start) - 24);
    let end = Math.min(count, Math.max(start + 48, lastVisibleRange.end + 25));
    if (Number(position) >= 0) { start = Math.min(start, Number(position)); end = Math.max(end, Number(position) + 1); }
    // A very distant focus need not make a viewport request unbounded.
    const focusedOutside = end - start > 512;
    if (focusedOutside) { start = Math.max(0, Math.min(count - 1, lastVisibleRange.start) - 24); end = Math.min(count, start + 256); }
    const fresh = count > 0 ? await getCurrentQueryFiles(start, Math.max(1, end - start)) : [];
    if (requestId !== currentContentRequestId) return;
    const loaded = new Map(fileList.value.filter(isRealFileItem).map(file => [Number(file.id), file]));
    const next = createVirtualFileSlots(count);
    fresh.forEach((file, index) => { next[start + index] = { ...loaded.get(Number(file.id)), ...file, isSelected: selectedFileIds.has(Number(file.id)) }; });
    if (focusedOutside && Number(position) >= 0 && Number(position) < count) next[Number(position)] = focus;
    currentContentRequestId++; currentThumbRequestId++; visibleRangeSeqId++;
    totalFileCount.value = count; totalFileSize.value = Number(result[1]); fileList.value = next;
    if (Number(position) >= 0) selectedItemIndex.value = Number(position);
    else if (selectedItemIndex.value >= count) selectedItemIndex.value = count - 1;
    if (selectMode.value) {
      captureSelectionForFileListRefresh();
      void restoreSelectionAfterFileListRefresh();
    }
    scheduleLayoutRefresh();
    const ready = fresh.map((_, index) => fileList.value[start + index]);
    void getFileListThumb(ready);
  } catch (e) { console.error('OCR results refresh failed:', e); }
  finally { ocrRefreshRunning = false; if (ocrRefreshAgain && !ocrContentDisposed) scheduleOcrContentRefresh(); }
}
async function refreshGroupedOcrContent(requestId: number) {
  const focus = fileList.value[selectedItemIndex.value];
  const position = focus?.id > 0 ? (await getGroupedFilePosition(getGroupingQueryParams(), focus.id)) ?? -1 : -1;
  if (requestId !== currentContentRequestId) return;
  if (focus?.id > 0 && position < 0 && showFilmstripLayout.value) { ocrPreviewDeferred.value = true; return; }
  ocrPreviewDeferred.value = false;
  let start = Math.max(0, lastVisibleRange.start - 24);
  const limit = Math.min(512, Math.max(80, lastVisibleRange.end - start + 25));
  let raw = await getCurrentGroupedQueryRows(start, limit);
  if (!raw) return;
  let result = normalizeGroupedRowsResult(raw);
  if (requestId !== currentContentRequestId) return;
  if (start >= result.totalRowCount && start > 0) {
    start = Math.max(0, result.totalRowCount - limit);
    raw = await getCurrentGroupedQueryRows(start, limit);
    if (!raw) return;
    result = normalizeGroupedRowsResult(raw);
  }
  if (requestId !== currentContentRequestId) return;
  const loaded = new Map(fileList.value.filter(isRealFileItem).map(file => [Number(file.id), file]));
  const next = createVirtualFileSlots(result.totalItemCount);
  const rows: any[] = Array.from({ length: result.totalRowCount }, (_, index) => ({ id: `grouped-ph-${index}`, isPlaceholder: true }));
  for (const group of result.groups) rows[group.rowIndex] = createGroupHeaderRow(group);
  result.rows.forEach((raw: any, index: number) => {
    const row = normalizeGridRow(raw, start + index);
    if (isItemRow(row) && row.file) {
      row.file = { ...loaded.get(Number(row.file.id)), ...row.file, isSelected: selectedFileIds.has(Number(row.file.id)) };
      next[row.file_index] = row.file;
    }
    rows[start + index] = row;
  });
  if (position >= 0 && position < next.length && !next[position]) next[position] = focus;
  if (selectMode.value) captureSelectionForFileListRefresh();
  currentContentRequestId++; currentThumbRequestId++; visibleRangeSeqId++;
  groupFileIdsCache.clear(); groupMetaMap.clear(); fileIdToGroupId.clear();
  groupSelectedCountMap.value = {}; groupSelectedSizeMap.value = {};
  for (const group of result.groups) groupMetaMap.set(group.groupId, { count: group.count, size: group.size });
  totalFileCount.value = result.totalItemCount; totalRowCount.value = result.totalRowCount; totalFileSize.value = result.totalSize;
  groupedTimelineGroups.value = result.groups; fileList.value = next; groupedRows.value = rows;
  selectedItemIndex.value = position >= 0 ? position : Math.min(selectedItemIndex.value, next.length - 1);
  if (selectMode.value) void restoreSelectionAfterFileListRefresh();
  scheduleLayoutRefresh();
  void getFileListThumb(result.rows.map((row: any) => next[Number(row.file_index ?? row.fileIndex)]).filter(Boolean));
}
onMounted(async () => {
  const off = await listen('ocr-index-updated', (event: any) => { if (config.main.sidebarIndex === SIDEBAR.OCR && event.payload?.libraryId === libConfig._libraryId) scheduleOcrContentRefresh(); });
  if (ocrContentDisposed) off(); else offOcrContent = off;
  const annotationOff = await listen('ocr-annotations-changed', (event:any) => { if (event.payload?.libraryId === libConfig._libraryId) scheduleOcrContentRefresh(); });
  if (ocrContentDisposed) annotationOff(); else offOcrAnnotations = annotationOff;
});
watch(showQuickView, open => { if (!open && ocrRefreshAgain) scheduleOcrContentRefresh(); });
watch(() => [showFilmstripLayout.value, selectedItemIndex.value], () => { if (ocrPreviewDeferred.value) scheduleOcrContentRefresh(); });
const objectContentStops: Array<() => void> = [];
onMounted(async () => {
  for (const name of ['object-index-updated','object-annotations-changed']) {
    const off = await listen(name,(event:any)=>{if(config.main.sidebarIndex===SIDEBAR.OBJECTS && event.payload?.libraryId===libConfig._libraryId)scheduleOcrContentRefresh();});
    if(ocrContentDisposed) off(); else objectContentStops.push(off);
  }
});
onBeforeUnmount(()=>objectContentStops.forEach(off=>off()));
onBeforeUnmount(() => { ocrContentDisposed = true; clearTimeout(ocrRefreshTimer); offOcrContent?.(); offOcrAnnotations?.(); });

const getCurrentQueryFileIds = () => {
  if (currentQuerySource.value === 'search') {
    return Promise.resolve([...currentSearchFileIds.value]);
  }
  if (currentQuerySource.value === 'collection' && currentCollectionId.value) {
    return getCollectionQueryFileIds(currentCollectionId.value, getGroupingQueryParams());
  }
  if (currentQuerySource.value === 'smart' && currentSmartQueryParams.value) {
    return getSmartQueryFileIds(getGroupingQueryParams());
  }
  return getQueryFileIds(getGroupingQueryParams());
};

async function refreshDedupSmartFileIds(requestId: number, params: any) {
  try {
    const fileIds = await getSmartQueryFileIds(params);
    if (requestId !== currentContentRequestId || currentQuerySource.value !== 'smart') return;
    dedupSmartFileIds.value = (Array.isArray(fileIds) ? fileIds : [])
      .map((id: any) => Number(id))
      .filter((id: number) => Number.isFinite(id) && id > 0);
  } catch (error) {
    console.error('refreshDedupSmartFileIds error:', error);
    if (requestId === currentContentRequestId && currentQuerySource.value === 'smart') {
      dedupSmartFileIds.value = [];
    }
  }
}

const getCurrentQueryFilePosition = async (fileId: number) => {
  if (currentQuerySource.value === 'search') {
    return currentSearchFileIds.value.findIndex(id => Number(id) === Number(fileId));
  }
  if (currentQuerySource.value === 'collection' && currentCollectionId.value) {
    const ids = await getCollectionQueryFileIds(currentCollectionId.value, currentQueryParams.value);
    return Array.isArray(ids) ? ids.findIndex((id: number) => Number(id) === Number(fileId)) : null;
  }
  if (currentQuerySource.value === 'smart' && currentSmartQueryParams.value) {
    return getSmartQueryFilePosition(currentSmartQueryParams.value, fileId);
  }
  return getQueryFilePosition(currentQueryParams.value, fileId);
};

async function fetchDataRange(start: number, end: number, reverse = false) {
  if (groupedModeActive.value) {
    return fetchGroupedRowsRange(start, end, reverse);
  }

  const requestId = currentContentRequestId;

  // Clamp range
  start = Math.max(0, start);
  end = Math.min(totalFileCount.value, end);
  
  if (start >= end || requestId !== currentContentRequestId) return;

  // Fetch in chunks
  // Keep normal browsing requests close to the actual viewport size. Large
  // fixed chunks make the initial "all files" view decode metadata and fetch
  // thumbnails for many items that are not yet visible.
  const chunkSize = Math.max(
    1,
    Math.min(selectionChunkSize.value, Math.max(32, visibleItemCount.value)),
  );
  const startChunk = Math.floor(start / chunkSize);
  const endChunk = Math.floor((end - 1) / chunkSize);
  const chunkPromises: Promise<void>[] = [];

  const chunkStartIdx = reverse ? endChunk : startChunk;
  const chunkEndIdx = reverse ? startChunk : endChunk;
  for (let i = chunkStartIdx; reverse ? i >= chunkEndIdx : i <= chunkEndIdx; reverse ? i-- : i++) {
    const chunkStart = i * chunkSize;
    const chunkEnd = Math.min(totalFileCount.value, chunkStart + chunkSize);
    
    // Check if this chunk still contains any placeholders.
    let chunkNeedsLoad = false;
    for (let idx = chunkStart; idx < chunkEnd; idx++) {
      if (isPendingFileItem(fileList.value[idx])) {
        chunkNeedsLoad = true;
        break;
      }
    }

    if (chunkNeedsLoad) {
      const key = `${requestId}:${chunkStart}-${chunkSize}`;
      if (pendingRequests.has(key)) {
        continue;
      }
      
      pendingRequests.add(key);
      
      const promise = getCurrentQueryFiles(chunkStart, chunkSize)
        .then(async (newFiles) => {
          if (requestId !== currentContentRequestId) return;
          if (newFiles) {
            // Update fileList and collect reactive references
            const filesToFetch = [];
            for (let j = 0; j < newFiles.length; j++) {
              if (requestId !== currentContentRequestId) return;
              if (chunkStart + j >= fileList.value.length) continue;

              const existingItem = fileList.value[chunkStart + j];
              // Avoid replacing already-loaded items; this prevents thumbnail flicker.
              if (existingItem && !existingItem.isPlaceholder) {
                continue;
              }

              // Preserve client-side state when upgrading placeholder -> real item.
              const fileId = Number(newFiles[j]?.id || 0);
              const isSelected = Boolean(existingItem?.isSelected) || (fileId > 0 && selectedFileIds.has(fileId));
              const rotate = existingItem ? (existingItem.rotate || 0) : 0;
              const thumbnail = existingItem?.thumbnail;

              fileList.value[chunkStart + j] = {
                ...newFiles[j],
                isSelected,
                rotate: rotate || newFiles[j].rotate || 0,
                thumbnail,
              };
              if (isSelected && fileId > 0 && !selectedFileIds.has(fileId)) {
                selectedFileIds.add(fileId);
                applySelectionDelta(fileList.value[chunkStart + j], 1);
              }

              // In scan streaming mode, total size starts at 0 and should grow
              // as placeholders are upgraded to real files.
              if (isScanStreamingMode.value) {
                totalFileSize.value += Number(newFiles[j]?.size || 0);
              }
              filesToFetch.push(fileList.value[chunkStart + j]);

              // Update ImageViewer if the selected file is loaded
              if (chunkStart + j === selectedItemIndex.value) {
                if (selectedItemIndex.value === 0) {
                  openImageViewer(selectedItemIndex.value, false, true);
                } else {
                  openImageViewer(selectedItemIndex.value, false);
                }
                updateSelectedImage(selectedItemIndex.value);
              }
            }
            // Trigger layout update as soon as metadata is available
            scheduleLayoutRefresh();
            // Fetch thumbnails for these files; await so the phase completes only when images are ready
            if (filesToFetch.length > 0) {
              if (reverse) filesToFetch.reverse();
              await getFileListThumb(filesToFetch);
            }
          }
        })
        .catch(err => {
            console.error(`Error fetching chunk ${key}:`, err);
            if(currentQuerySource.value==='smart' && requestId===currentContentRequestId)smartQueryError.value=String(err);
        })
        .finally(() => {
          pendingRequests.delete(key);
        });

      chunkPromises.push(promise);
    } else {
        // console.log(`Chunk already loaded or invalid: ${chunkStart}`);
    }
  }

  if (chunkPromises.length > 0) {
    await Promise.all(chunkPromises);
  }
}

async function fetchGroupedRowsRange(start: number, end: number, reverse = false) {
  const requestId = currentContentRequestId;

  start = Math.max(0, start);
  end = Math.min(totalRowCount.value, end);
  if (start >= end || requestId !== currentContentRequestId) return;

  const chunkSize = selectionChunkSize.value;
  const startChunk = Math.floor(start / chunkSize);
  const endChunk = Math.floor((end - 1) / chunkSize);
  const chunkPromises: Promise<void>[] = [];

  const chunkStartIdx = reverse ? endChunk : startChunk;
  const chunkEndIdx = reverse ? startChunk : endChunk;
  for (let i = chunkStartIdx; reverse ? i >= chunkEndIdx : i <= chunkEndIdx; reverse ? i-- : i++) {
    const chunkStart = i * chunkSize;
    const chunkEnd = Math.min(totalRowCount.value, chunkStart + chunkSize);
    let chunkNeedsLoad = false;
    for (let idx = chunkStart; idx < chunkEnd; idx++) {
      if (groupedRows.value[idx]?.isPlaceholder) {
        chunkNeedsLoad = true;
        break;
      }
    }
    if (!chunkNeedsLoad) continue;

    const key = `${requestId}:grouped:${chunkStart}-${chunkSize}`;
    if (pendingRequests.has(key)) continue;
    pendingRequests.add(key);

    const promise = getCurrentGroupedQueryRows(chunkStart, chunkEnd - chunkStart)
      .then(async (result) => {
        if (requestId !== currentContentRequestId) return;
        const normalized = normalizeGroupedRowsResult(result);
        const filesToFetch = applyGroupedRows(normalized.rows, chunkStart);

        scheduleLayoutRefresh();
        if (filesToFetch.length > 0) {
          if (reverse) filesToFetch.reverse();
          await getFileListThumb(filesToFetch);
        }
      })
      .catch(err => {
        console.error(`Error fetching grouped chunk ${key}:`, err);
      })
      .finally(() => {
        pendingRequests.delete(key);
      });

    chunkPromises.push(promise);
  }

  if (chunkPromises.length > 0) {
    await Promise.all(chunkPromises);
  }
}

function normalizeGridRow(row: any, rowIndex: number) {
  if (!row) {
    return {
      id: `grouped-missing-${rowIndex}`,
      isPlaceholder: true,
    };
  }
  if (row?.type === 'group') {
    const groupId = String(row.group_id ?? row.groupId ?? row.id ?? `group-${rowIndex}`);
    return {
      ...row,
      type: 'group',
      id: row.row_id ?? row.rowId ?? `group-row-${groupId}`,
      group_id: groupId,
      label: formatGroupLabel(row.label ?? ''),
      count: Number(row.count || 0),
    };
  }

  const file = row?.file ?? row;
  const fileIndex = Number(row?.file_index ?? row?.fileIndex ?? rowIndex);
  const groupId = row?.group_id ?? row?.groupId;
  return {
    ...row,
    type: 'item',
    id: row?.row_id ?? row?.rowId ?? `item-row-${file?.id ?? rowIndex}`,
    group_id: groupId ? String(groupId) : '',
    file_index: fileIndex,
    file,
  };
}

function applyGroupedRows(rows: any[], rowOffset: number) {
  const filesToFetch: any[] = [];
  for (let j = 0; j < rows.length; j++) {
    const rowIndex = rowOffset + j;
    if (rowIndex >= groupedRows.value.length) continue;
    const row = normalizeGridRow(rows[j], rowIndex);

    if (isGroupRow(row)) {
      groupedRows.value[rowIndex] = row;
      const groupId = String(row.group_id || '');
      if (groupId) {
        groupMetaMap.set(groupId, {
          count: Number(row.count || 0),
          size: Number(row.size || 0),
        });
        if (groupSelectedCountMap.value[groupId] === undefined) {
          updateGroupSelectedCount(groupId, 0);
        }
        if (groupSelectedSizeMap.value[groupId] === undefined) {
          updateGroupSelectedSize(groupId, 0);
        }
      }
      continue;
    }

    if (isItemRow(row)) {
      const fileIndex = Number(row.file_index);
      const file = row.file;
      if (!Number.isFinite(fileIndex) || fileIndex < 0 || fileIndex >= fileList.value.length || !file) {
        groupedRows.value[rowIndex] = row;
        continue;
      }

      const existingItem = fileList.value[fileIndex];
      const fileId = Number(file?.id || 0);
      const isSelected = Boolean(existingItem?.isSelected) || (fileId > 0 && selectedFileIds.has(fileId));
      const rotate = existingItem ? (existingItem.rotate || 0) : 0;
      const thumbnail = existingItem?.thumbnail;
      const nextFile = {
        ...file,
        isSelected,
        rotate: rotate || file.rotate || 0,
        thumbnail,
      };
      fileList.value[fileIndex] = nextFile;
      row.file = fileList.value[fileIndex];
      groupedRows.value[rowIndex] = row;
      if (isSelected && fileId > 0 && !selectedFileIds.has(fileId)) {
        selectedFileIds.add(fileId);
        applySelectionDelta(fileList.value[fileIndex], 1);
      }

      const groupId = String(row.group_id || '');
      if (groupId && fileId) fileIdToGroupId.set(fileId, groupId);
      filesToFetch.push(fileList.value[fileIndex]);

      if (fileIndex === selectedItemIndex.value) {
        openImageViewer(selectedItemIndex.value, false, selectedItemIndex.value === 0);
        updateSelectedImage(selectedItemIndex.value);
      }
    }
  }
  return filesToFetch;
}

async function initializeGroupedFileList(requestId: number) {
  if (!isGroupingSupportedForCurrentView()) {
    resetGroupingState();
    return false;
  }

  if (effectiveGroupBy.value === GROUP.FOLDER) {
    await ensureFolderGroupRoots();
  }

  const groupedResult = await getCurrentGroupedQueryRows(0, selectionChunkSize.value);
  if (requestId !== currentContentRequestId) return true;
  if (!groupedResult) {
    resetGroupingState();
    return false;
  }

  const normalized = normalizeGroupedRowsResult(groupedResult);
  // Invalidate the old viewport before replacing rows, so an unchanged range
  // still hydrates the new placeholders when the virtual scroller updates.
  lastVisibleRange = { start: -1, end: -1 };
  visibleRangeSeqId++;
  clearSelectionForFileListUpdate();
  groupedModeActive.value = true;
  groupedTimelineGroups.value = normalized.groups;
  totalFileCount.value = normalized.totalItemCount;
  countedFolderRequestId.value = requestId;
  totalRowCount.value = normalized.totalRowCount;
  totalFileSize.value = normalized.totalSize;
  contentCountIsAuthoritative.value = true;
  scrollPosition.value = 0;
  timelineData.value = [];
  groupedRows.value = Array.from({ length: totalRowCount.value }).map((_, i) => ({
    id: `grouped-ph-${i}`,
    isPlaceholder: true,
  }));

  for (const group of normalized.groups) {
    const rowIndex = Number(group.rowIndex || 0);
    if (rowIndex < 0 || rowIndex >= groupedRows.value.length) continue;
    groupedRows.value[rowIndex] = createGroupHeaderRow(group);
    if (group.groupId) {
      groupMetaMap.set(group.groupId, {
        count: Number(group.count || 0),
        size: Number(group.size || 0),
      });
      updateGroupSelectedCount(group.groupId, 0);
      updateGroupSelectedSize(group.groupId, 0);
    }
  }

  fileList.value = Array.from({ length: totalFileCount.value }).map((_, i) => ({
    id: `ph-${i}`,
    isPlaceholder: true,
    name: '',
    size: 0,
  }));

  const initialFiles = applyGroupedRows(normalized.rows, 0);
  if (initialFiles.length > 0) {
    await getFileListThumb(initialFiles);
  }
  await nextTick();
  gridViewRef.value?.refreshLayout?.();
  markDedupSourceUpdated(requestId);
  restoreInitialSelectionIfNeeded();
  restoreScrollAfterRefresh();
  if (totalFileCount.value === 0) {
    openImageViewer(0, false, true);
  }
  return true;
}

async function hydrateRangeForSelection(startIndex: number, endIndex: number) {
  if (fileList.value.length === 0 || endIndex <= startIndex) return true;
  const requestId = currentContentRequestId;
  const libraryId = libConfig._libraryId;
  const files = fileList.value;
  const isCurrent = () => requestId === currentContentRequestId
    && libraryId === libConfig._libraryId && files === fileList.value;
  const chunkSize = selectionChunkSize.value;
  const cappedStart = Math.max(0, Math.min(startIndex, fileList.value.length));
  const cappedEnd = Math.max(cappedStart, Math.min(endIndex, fileList.value.length));
  let hydratedPlaceholders = false;
  isProcessing.value = true;

  try {
    const firstChunkStart = Math.floor(cappedStart / chunkSize) * chunkSize;
    for (let chunkStart = firstChunkStart; chunkStart < cappedEnd; chunkStart += chunkSize) {
      if (!isCurrent()) return false;
      const chunkEnd = Math.min(cappedEnd, chunkStart + chunkSize);
      const selectionStart = Math.max(cappedStart, chunkStart);
      const needsLoad = fileList.value
        .slice(selectionStart, chunkEnd)
        .some(isPendingFileItem);
      if (!needsLoad) continue;

      const loadedFiles = await getCurrentQueryFiles(chunkStart, chunkSize);
      if (!isCurrent()) return false;
      if (!loadedFiles || loadedFiles.length === 0) return false;
      hydratedPlaceholders = true;

      for (let i = 0; i < loadedFiles.length; i++) {
        const targetIndex = chunkStart + i;
        if (targetIndex >= fileList.value.length || targetIndex >= cappedEnd) break;
        if (targetIndex < cappedStart) continue;
        const existingItem = fileList.value[targetIndex];
        const fileId = Number(loadedFiles[i]?.id || 0);
        const isSelected = Boolean(existingItem?.isSelected) || (fileId > 0 && selectedFileIds.has(fileId));
        fileList.value[targetIndex] = {
          ...loadedFiles[i],
          isSelected,
          rotate: existingItem?.rotate || loadedFiles[i].rotate || 0,
        };
        if (isSelected && fileId > 0 && !selectedFileIds.has(fileId)) {
          selectedFileIds.add(fileId);
          applySelectionDelta(fileList.value[targetIndex], 1);
        }
      }
      if (fileList.value.slice(selectionStart, chunkEnd).some(isPendingFileItem)) {
        return false;
      }
    }
    return isCurrent();
  } catch (err) {
    console.error('hydrateRangeForSelection error:', err);
    return false;
  } finally {
    if (isCurrent()) {
      isProcessing.value = false;
      if (hydratedPlaceholders) scheduleLayoutRefresh();
    }
  }
}

async function hydrateSelectedFilesByIds() {
  if (selectedFileIds.size === 0) return getActionableSelectedItems();

  const loadedSelectedItems = getActionableSelectedItems();
  const loadedIds = new Set(
    loadedSelectedItems
      .map(item => Number(item.id || 0))
      .filter(id => id > 0),
  );
  const missingIds = Array.from(selectedFileIds).filter(id => !loadedIds.has(id));
  if (missingIds.length === 0) return loadedSelectedItems;

  const hydratedItems: any[] = [...loadedSelectedItems];
  const missingIdSet = new Set(missingIds);
  const chunkSize = Math.max(1, selectionChunkSize.value);
  isProcessing.value = true;
  try {
    for (let start = 0; start < totalFileCount.value && missingIdSet.size > 0; start += chunkSize) {
      const files = await getCurrentQueryFiles(start, chunkSize);
      if (!Array.isArray(files)) return null;

      for (const file of files) {
        const fileId = Number(file?.id || 0);
        if (fileId <= 0 || !missingIdSet.has(fileId)) continue;
        hydratedItems.push({
          ...file,
          isSelected: true,
          rotate: file.rotate || 0,
        });
        missingIdSet.delete(fileId);
      }
    }
    return missingIdSet.size === 0 ? hydratedItems : null;
  } catch (err) {
    console.error('hydrateSelectedFilesByIds error:', err);
    return null;
  } finally {
    isProcessing.value = false;
  }
}

async function getActionableSelectedItemsForAction() {
  if (selectedFileIds.size > getActionableSelectedItems().length) {
    const hydratedItems = await hydrateSelectedFilesByIds();
    if (!hydratedItems) {
      toast.error(t('info_panel.selection_load_failed'));
      return null;
    }
    return hydratedItems;
  }

  const hasSelectedPlaceholders = fileList.value.some(
    item => item?.isSelected && item?.isPlaceholder,
  );
  if (hasSelectedPlaceholders && !await hydrateRangeForSelection(0, fileList.value.length)) {
    toast.error(t('info_panel.selection_load_failed'));
    return null;
  }
  return getActionableSelectedItems();
}

async function getSelectedItemsForClipboard(limit = 10) {
  if (selectedFileIds.size > 0 && getActionableSelectedItems().length < Math.min(selectedFileIds.size, limit)) {
    const hydratedItems = await hydrateSelectedFilesByIds();
    if (!hydratedItems) {
      toast.error(t('info_panel.selection_load_failed'));
      return null;
    }
    return hydratedItems.slice(0, limit);
  }

  const selectedItems: any[] = [];
  const chunkSize = Math.max(1, selectionChunkSize.value);

  for (let chunkStart = 0; chunkStart < fileList.value.length && selectedItems.length < limit; chunkStart += chunkSize) {
    const chunkEnd = Math.min(fileList.value.length, chunkStart + chunkSize);
    const chunk = fileList.value.slice(chunkStart, chunkEnd);
    if (chunk.some(item => item?.isSelected && item?.isPlaceholder)) {
      if (!await hydrateRangeForSelection(chunkStart, chunkEnd)) {
        toast.error(t('info_panel.selection_load_failed'));
        return null;
      }
    }
    for (let index = chunkStart; index < chunkEnd && selectedItems.length < limit; index++) {
      const item = fileList.value[index];
      if (item?.isSelected && isRealFileItem(item)) {
        selectedItems.push(item);
      }
    }
  }

  return selectedItems;
}

// Track last visible range to avoid redundant fetches
let lastVisibleRange = { start: -1, end: -1 };
let visibleRangeSeqId = 0;

async function fetchMissingVisibleThumbnails(startIndex: number, endIndex: number) {
  const rows = groupedModeActive.value
    ? gridRows.value.slice(startIndex, endIndex + 1)
    : fileList.value.slice(startIndex, endIndex + 1);
  if (config.main.sidebarIndex === SIDEBAR.SEARCH) {
    const visible = rows.map((row: any) => isItemRow(row) ? row.file : row).filter((file: any) => isRealFileItem(file) && !file._ocr);
    if (visible.length) {
      const requestId = currentContentRequestId;
      void enrichOcrPhotos(visible).then(enriched => { if (requestId === currentContentRequestId) enriched.forEach((file, index) => { visible[index]._ocr = file._ocr; }); });
    }
  }
  const seen = new Set<number>();
  const files = rows
    .map((row: any) => isItemRow(row) ? row.file : row)
    .filter((file: any) => {
      if (!isRealFileItem(file) || (file.thumbnail && !file.rawThumbnailStale)) return false;
      const fileId = Number(file.id || 0);
      if (fileId <= 0 || seen.has(fileId)) return false;
      seen.add(fileId);
      return true;
    });

  if (files.length > 0) {
    await getFileListThumb(files);
  }
}

function handleVisibleRangeUpdate({ startIndex, endIndex }: { startIndex: number, endIndex: number }) {
  const unchanged = lastVisibleRange.start === startIndex && lastVisibleRange.end === endIndex;
  lastVisibleRange = { start: startIndex, end: endIndex };
  refreshThumbnailViewport();
  // Skip if the range hasn't changed significantly
  if (unchanged) {
    void fetchMissingVisibleThumbnails(startIndex, endIndex);
    return;
  }

  const buffer = Math.max(40, Math.min(visibleItemCount.value, 120));
  const seqId = ++visibleRangeSeqId;

  // Phase 1: viewport thumbnails first (immediately visible)
  fetchDataRange(startIndex, endIndex + 1).then(() => {
    if (seqId !== visibleRangeSeqId) return;
    fetchMissingVisibleThumbnails(startIndex, endIndex).then(() => {
      if (seqId !== visibleRangeSeqId) return;

      // Phase 2: below viewport (most likely scroll direction)
      fetchDataRange(endIndex + 1, endIndex + 1 + buffer).then(() => {
        if (seqId !== visibleRangeSeqId) return;

        // Phase 3: above viewport (least likely, reverse: load closest to viewport first)
        fetchDataRange(Math.max(0, startIndex - buffer), startIndex, true);
      });
    });
  });
}

// get file list 
async function getFileList(
  {
    searchFileType = config.search.fileType, 
    sortType = config.search.sortType, 
    sortOrder = config.search.sortOrder, 
    searchFileName = '', 
    searchAllSubfolders = '',
    searchFolder = '', 
    startDate = 0,
    endDate = 0,
    calendarSort = config.settings.calendarSort,
    folderSort = config.settings.folderSort,
    categorySort = categorySortForGroup(effectiveGroupBy.value),
    make = '',
    model = '', 
    lensMake = '',
    lensModel = '',
    locationNodeId = 0,
    isFavorite = false, 
    rating = -1,
    cullingFlag = -1,
    tagId = 0,
    tagGroupId = 0,
    personId = 0,
    ocrState = '',
    ocrText = '',
    objectState = '',
    objectClass = '',
    objectMinCount = 1,
    gpsMinLat = null,
    gpsMaxLat = null,
    gpsMinLon = null,
    gpsMaxLon = null
  } = {},
  requestId: number,
  sourceContext: { source: 'collection' | 'smart'; collectionId?: number | null; smartParams?: any } | null = null,
  retainedRandomSeed?: ReturnType<typeof createRandomSeed>,
) { 
  const randomSeed = retainedRandomSeed ?? createRandomSeed();
  currentQuerySource.value = sourceContext?.source || 'query';
  currentSmartQueryParams.value = sourceContext?.source === 'smart'
    ? { ...sourceContext.smartParams, randomSeed }
    : null;
  currentCollectionId.value = sourceContext?.source === 'collection' ? sourceContext.collectionId || null : null;
  currentSearchFileIds.value = [];

  // Update current query params with all fields
  currentQueryParams.value = {
    searchFileType,
    sortType,
    sortOrder,
    randomSeed,
    searchFileName,
    searchAllSubfolders,
    searchFolder,
    startDate,
    endDate,
    calendarSort,
    folderSort,
    categorySort,
    make,
    model,
    lensMake,
    lensModel,
    locationNodeId,
    isFavorite,
    rating,
    cullingFlag,
    tagId,
    tagGroupId,
    personId,
    ocrState,
    ocrText,
    objectState, objectClass, objectMinCount,
    gpsMinLat,
    gpsMaxLat,
    gpsMinLon,
    gpsMaxLon,
  };

  if (sourceContext?.source === 'smart') {
    void refreshDedupSmartFileIds(requestId, currentSmartQueryParams.value);
  }

  // Set loading state
  isLoading.value = true;

  try {
    if (await initializeGroupedFileList(requestId)) return;

    // Get count and sum first
    const result = await getCurrentQueryCountAndSum();
    
    // Check if the request is still valid. 
    if (requestId !== currentContentRequestId) {
      return;
    }

    if (result) {
      clearSelectionForFileListUpdate();
      totalFileCount.value = result[0];
      countedFolderRequestId.value = requestId;
      totalFileSize.value = result[1];
      contentCountIsAuthoritative.value = true;
      
      // Get timeline data for date-based sorts
      getCurrentQueryTimeLine().then(data => {
        if (requestId === currentContentRequestId) {
          timelineData.value = data;
        }
      }).catch(error=>{if(currentQuerySource.value==='smart')smartQueryError.value=String(error);});
      
      // Initialize fileList with placeholders
      fileList.value = createVirtualFileSlots(totalFileCount.value);
      markDedupSourceUpdated(requestId);
      restoreInitialSelectionIfNeeded();
      restoreScrollAfterRefresh();
      if (totalFileCount.value === 0) {
        openImageViewer(0, false, true);
      }
      
      // Reset visible range tracking when changing views
      lastVisibleRange = { start: -1, end: -1 };
      visibleRangeSeqId++;
    } else {
      clearSelectionForFileListUpdate();
      clearContentRows();
      markDedupSourceUpdated(requestId);
      openImageViewer(0, false, true);
    }
  } catch (err) {
    console.error('getFileList error:', err);
    if (requestId === currentContentRequestId) {
      clearSelectionForFileListUpdate();
      clearContentRows();
      markDedupSourceUpdated(requestId);
      openImageViewer(0, false, true);
    }
  } finally {
    // Only clear loading state if this is still the active request
    if (requestId === currentContentRequestId) {
      isLoading.value = false;
      hasLoadedInitialResult.value = true;
      contentReady.value = true;
    }
  }
}

async function getMapSearchClusterFileList(fileIds: number[], gpsParams: Record<string, any>, requestId: number, exactMembers = false) {
  currentMapSearchExactMembers = exactMembers;
  currentQuerySource.value = 'search';
  currentSmartQueryParams.value = null;
  currentCollectionId.value = null;
  currentQueryParams.value = gpsParams;
  isLoading.value = true;

  try {
    const files = await getFilesByIds(fileIds);
    if (requestId !== currentContentRequestId) return;

    const orderedIds = new Map(fileIds.map((id, index) => [Number(id), index]));
    const inCluster = (files || []).sort((a, b) => (orderedIds.get(Number(a.id)) ?? Infinity) - (orderedIds.get(Number(b.id)) ?? Infinity)).filter((file: any) => {
      if (!passesAlbumFilters(file)) return false;
      if (exactMembers) return true;
      if (file.gps_latitude == null || file.gps_longitude == null || file.gps_latitude === '' || file.gps_longitude === '') return false;
      const lat = Number(file.gps_latitude);
      const lon = Number(file.gps_longitude);
      if (!Number.isFinite(lat) || !Number.isFinite(lon)) return false;
      const lonMatches = gpsParams.gpsMinLon <= gpsParams.gpsMaxLon
        ? lon >= gpsParams.gpsMinLon && lon <= gpsParams.gpsMaxLon
        : lon >= gpsParams.gpsMinLon || lon <= gpsParams.gpsMaxLon;
      return lat >= gpsParams.gpsMinLat && lat <= gpsParams.gpsMaxLat && lonMatches;
    });

    clearSelectionForFileListUpdate();
    resetGroupingState();
    fileList.value = preserveLoadedThumbnails(inCluster);
    selectedItemIndex.value = inCluster.length > 0 ? 0 : -1;
    if (inCluster.length > 0) {
      void getFileListThumb([fileList.value[0]], { viewportBound: false });
    }
    currentSearchFileIds.value = inCluster
      .map((file: any) => Number(file.id))
      .filter((id: number) => Number.isFinite(id) && id > 0);
    totalFileCount.value = inCluster.length;
    totalFileSize.value = inCluster.reduce((total: number, file: any) => total + Number(file.size || 0), 0);
    timelineData.value = [];
    markDedupSourceUpdated(requestId);
    restoreInitialSelectionIfNeeded();
    openImageViewer(0, false, true);
    lastVisibleRange = { start: -1, end: -1 };
    visibleRangeSeqId++;
  } catch (error) {
    console.error('getMapSearchClusterFileList error:', error);
    if (requestId === currentContentRequestId) showEmptyContent(requestId);
  } finally {
    if (requestId === currentContentRequestId) {
      isLoading.value = false;
      hasLoadedInitialResult.value = true;
      contentReady.value = true;
    }
  }
}

async function getCollectionFileList(collectionId: number, requestId: number) {
  currentQuerySource.value = 'collection';
  currentCollectionId.value = collectionId;
  currentSmartQueryParams.value = null;
  currentSearchFileIds.value = [];
  currentQueryParams.value = {
    ...currentQueryParams.value,
    searchFileType: config.search.fileType,
    sortType: config.search.sortType,
    sortOrder: config.search.sortOrder,
    randomSeed: createRandomSeed(),
    searchFileName: '',
    searchAllSubfolders: '',
    searchFolder: '',
    startDate: 0,
    endDate: 0,
    calendarSort: config.settings.calendarSort,
    folderSort: config.settings.folderSort,
    categorySort: categorySortForGroup(effectiveGroupBy.value),
    make: '',
    model: '',
    lensMake: '',
    lensModel: '',
    locationNodeId: 0,
    isFavorite: false,
    rating: -1,
    tagId: 0,
    tagGroupId: 0,
    personId: 0,
  };

  isLoading.value = true;

  try {
    if (await initializeGroupedFileList(requestId)) return;

    const result = await getCurrentQueryCountAndSum();
    if (requestId !== currentContentRequestId) return;

    if (result) {
      clearSelectionForFileListUpdate();
      resetGroupingState();
      totalFileCount.value = result[0];
      totalFileSize.value = result[1];
      contentCountIsAuthoritative.value = true;
      timelineData.value = [];

      fileList.value = createVirtualFileSlots(totalFileCount.value);
      markDedupSourceUpdated(requestId);
      restoreInitialSelectionIfNeeded();
      restoreScrollAfterRefresh();
      if (totalFileCount.value === 0) {
        openImageViewer(0, false, true);
      }

      lastVisibleRange = { start: -1, end: -1 };
      visibleRangeSeqId++;
    } else {
      clearSelectionForFileListUpdate();
      clearContentRows();
      markDedupSourceUpdated(requestId);
      openImageViewer(0, false, true);
    }
  } catch (err) {
    console.error('getCollectionFileList error:', err);
    if (requestId === currentContentRequestId) {
      clearSelectionForFileListUpdate();
      clearContentRows();
      markDedupSourceUpdated(requestId);
      openImageViewer(0, false, true);
    }
  } finally {
    if (requestId === currentContentRequestId) {
      isLoading.value = false;
      hasLoadedInitialResult.value = true;
      contentReady.value = true;
    }
  }
}

async function getSmartFileList(smartAlbum: any, requestId: number) {
  const query = smartAlbum?.query;
  const rules = Array.isArray(query?.rules) ? query.rules : [];
  if (!query || rules.length === 0) {
    showEmptyContent(requestId);
    return;
  }

  smartQueryError.value='';
  currentQuerySource.value = 'smart';
  currentCollectionId.value = null;
  currentSearchFileIds.value = [];
  dedupSmartFileIds.value = null;
  currentSmartQueryParams.value = {
    ...smartQueryParams(smartAlbum),
    version: Number(query.version || 1),
    match: query.match === 'any' ? 'any' : 'all',
    rules,
    sortType: Number(smartAlbum?.sort?.type ?? 0),
    sortOrder: Number(smartAlbum?.sort?.order ?? 1),
    randomSeed: currentSmartQueryParams.value?.randomSeed ?? createRandomSeed(),
    folderSort: Number(config.settings.folderSort || 0),
    calendarSort: Number(config.settings.calendarSort || 0),
    categorySort: categorySortForGroup(effectiveGroupBy.value),
    groupBy: effectiveGroupBy.value,
  };
  void refreshDedupSmartFileIds(requestId, currentSmartQueryParams.value);
  void updateSmartAlbumCover(smartAlbum, requestId).catch(error=>{if(requestId===currentContentRequestId)smartQueryError.value=String(error);});

  isLoading.value = true;

  try {
    if (await initializeGroupedFileList(requestId)) return;

    const result = await getCurrentQueryCountAndSum();
    if (requestId !== currentContentRequestId) return;

    if (result) {
      clearSelectionForFileListUpdate();
      totalFileCount.value = result[0];
      totalFileSize.value = result[1];
      contentCountIsAuthoritative.value = true;

      getCurrentQueryTimeLine().then(data => {
        if (requestId === currentContentRequestId) {
          timelineData.value = data;
        }
      }).catch(error=>{if(requestId===currentContentRequestId)smartQueryError.value=String(error);});

      fileList.value = createVirtualFileSlots(totalFileCount.value);
      markDedupSourceUpdated(requestId);
      restoreInitialSelectionIfNeeded();
      restoreScrollAfterRefresh();
      if (totalFileCount.value === 0) {
        openImageViewer(0, false, true);
      }

      lastVisibleRange = { start: -1, end: -1 };
      visibleRangeSeqId++;
    } else {
      clearSelectionForFileListUpdate();
      clearContentRows();
      markDedupSourceUpdated(requestId);
      openImageViewer(0, false, true);
    }
  } catch (err) {
    console.error('getSmartFileList error:', err);
    if(requestId===currentContentRequestId)smartQueryError.value=String(err);
    if (requestId === currentContentRequestId) {
      clearSelectionForFileListUpdate();
      clearContentRows();
      markDedupSourceUpdated(requestId);
      openImageViewer(0, false, true);
    }
  } finally {
    if (requestId === currentContentRequestId) {
      isLoading.value = false;
      hasLoadedInitialResult.value = true;
      contentReady.value = true;
    }
  }
}

async function updateSmartAlbumCover(smartAlbum: any, requestId: number) {
  const result = await getSmartQueryFiles(currentSmartQueryParams.value, 0, 1);
  if (
    requestId !== currentContentRequestId ||
    libConfig.smartAlbum.type !== 'custom' ||
    String(libConfig.smartAlbum.id) !== String(smartAlbum?.id)
  ) return;

  const firstFile = Array.isArray(result) ? result[0] : result?.files?.[0];
  const coverFileId = Number(firstFile?.id || 0) || null;
  const albums = [...(libConfig.smartAlbums || [])];
  const albumIndex = albums.findIndex((album: any) => String(album.id) === String(smartAlbum?.id));
  if (albumIndex < 0 || Number(albums[albumIndex].coverFileId || 0) === Number(coverFileId || 0)) return;

  albums[albumIndex] = {
    ...albums[albumIndex],
    coverFileId,
  };
  libConfig.smartAlbums = albums;
}

async function getImageSearchFileList(
  searchText: string,
  fileId: number,
  requestId: number,
  thresholdOverride?: number,
) {
  const thresholdIndex = config.settings.imageSearch.thresholdIndex;
  const threshold = thresholdOverride
    ?? (fileId > 0
      ? config.similarImageSearchThreshold
      : config.imageSearchThresholds[thresholdIndex]);
  currentQuerySource.value = 'search';
  currentSmartQueryParams.value = null;
  currentImageSearchParams.value = {
    searchText,
    fileId,
    threshold,
    fileType: tempViewMode.value === 'similar' ? 0 : Number(config.search.fileType || 0),
  };
  currentCollectionId.value = null;
  currentSearchFileIds.value = [];
  imageSearchError.value = false;
  imageSearchLanguageUnsupported.value = false;

  // set loading state
  isLoading.value = true;
  timelineData.value = []; // reset timeline data

  try {
    // Check if the request is still valid. 
    if (requestId !== currentContentRequestId) {
      return;
    }

    const result = await searchSimilarImages(currentImageSearchParams.value);
    if (requestId !== currentContentRequestId) return;

    if (result) {
      clearSelectionForFileListUpdate();
      resetGroupingState();
      fileList.value = preserveLoadedThumbnails(result.filter(passesAlbumFilters));
      contentCountIsAuthoritative.value = true;
      currentSearchFileIds.value = fileList.value
        .map(file => Number(file.id))
        .filter(id => Number.isFinite(id) && id > 0);
      totalFileCount.value = fileList.value.length;
      totalFileSize.value = fileList.value.reduce((total, file) => total + file.size, 0);
      markDedupSourceUpdated(requestId);
      restoreInitialSelectionIfNeeded();
      openImageViewer(0, false, true);

      // Reset visible range tracking when changing views
      lastVisibleRange = { start: -1, end: -1 };
      visibleRangeSeqId++;

      // Thumbnail loading is driven by visible-range updates. Avoid requesting
      // every semantic-search result before the user scrolls to it.
    } else {
      clearSelectionForFileListUpdate();
      clearContentRows();
      totalFileCount.value = 0;
      totalFileSize.value = 0;
      markDedupSourceUpdated(requestId);
    }
  } catch (err) {
    console.error('getImageSearchFileList error:', err);
    if (requestId === currentContentRequestId) {
      imageSearchError.value = true;
      clearSelectionForFileListUpdate();
      clearContentRows();
      totalFileCount.value = 0;
      totalFileSize.value = 0;
      markDedupSourceUpdated(requestId);
    }
  } finally {
    // Only clear loading state if this is still the active request
    if (requestId === currentContentRequestId) {
      isLoading.value = false;
      hasLoadedInitialResult.value = true;
      contentReady.value = true;
    }
  }
}

async function resolveObjectText(searchText: string): Promise<string[]> {
  const query = searchText.trim();
  if (!query) return [];
  try {
    const state: any = await invoke('get_object_state', {
      libraryId: libConfig._libraryId,
    });
    return resolveObjectTextClasses(query, state?.classes || [], t);
  } catch {
    return [];
  }
}

async function getUnifiedSearchFileList(searchText: string, requestId: number) {
  if (mediaAiSearch.active && mediaAiSearch.libraryId === libConfig._libraryId) {
    await getMediaAiSearchFileList(searchText, requestId); return;
  }
  const textParams = {
    searchMetadata: true,
    searchFileType: config.search.fileType,
    sortType: 3,
    sortOrder: 0,
    searchFileName: searchText,
    searchAllSubfolders: '',
    searchFolder: '',
    startDate: 0,
    endDate: 0,
    calendarSort: config.settings.calendarSort,
    folderSort: config.settings.folderSort,
    categorySort: categorySortForGroup(effectiveGroupBy.value),
    make: '',
    model: '',
    lensMake: '',
    lensModel: '',
    locationNodeId: 0,
    isFavorite: false,
    rating: -1,
    tagId: 0,
    tagGroupId: 0,
    personId: 0,
    groupBy: 0,
  };
  currentQueryParams.value = textParams;
  currentQuerySource.value = 'search';
  currentSmartQueryParams.value = null;
  currentCollectionId.value = null;
  currentSearchFileIds.value = [];
  currentImageSearchParams.value = {
    searchText,
    fileId: 0,
    threshold: config.imageSearchThresholds[config.settings.imageSearch.thresholdIndex],
    fileType: Number(config.search.fileType || 0),
  };
  const objectClasses = await resolveObjectText(searchText);
  const objectParams = { ...textParams, searchFileName: '', objectClasses };
  imageSearchError.value = false;
  const isDefaultModelUnsupportedLanguage =
    (config.settings.ai?.semanticLanguages?.length === 1 && config.settings.ai.semanticLanguages[0] === 'en')
    && hasNonLatinLetters(searchText);
  imageSearchLanguageUnsupported.value = isDefaultModelUnsupportedLanguage;
  isLoading.value = true;
  timelineData.value = [];
  let visualSearchFailed = false;

  try {
    const [textResult, objectResult, visualResult] = await Promise.all([
      getQueryFiles(textParams, 0, 0).catch(error => {
        console.error('Text search failed:', error);
        return [];
      }),
      objectClasses.length
        ? getQueryFiles(objectParams, 0, 0).catch(error => {
            console.error('Object search failed:', error);
            return [];
          })
        : Promise.resolve([]),
      (isDefaultModelUnsupportedLanguage
        ? Promise.resolve([])
        : searchSimilarImages(currentImageSearchParams.value)
      ).catch(error => {
        console.error('Visual search failed:', error);
        visualSearchFailed = true;
        return [];
      }),
    ]);
    if (requestId !== currentContentRequestId) return;
    imageSearchError.value = visualSearchFailed;

    const textMatches = Array.isArray(textResult) ? textResult : [];
    const textIds = new Set(textMatches.map((file: any) => Number(file.id)));
    const objectMatches = (Array.isArray(objectResult) ? objectResult : [])
      .filter((file: any) => !textIds.has(Number(file.id)));
    for (const file of objectMatches) (file as any)._object = true;
    const objectIds = new Set(objectMatches.map((file: any) => Number(file.id)));
    const visualMatches = (Array.isArray(visualResult) ? visualResult : [])
      .filter((file: any) => !textIds.has(Number(file.id)))
      .filter((file: any) => !objectIds.has(Number(file.id)))
      .filter(passesAlbumFilters);
    const files = preserveLoadedThumbnails([...textMatches, ...objectMatches, ...visualMatches]);
    contentCountIsAuthoritative.value = true;

    clearSelectionForFileListUpdate();
    resetGroupingState();
    fileList.value = files;
    currentSearchFileIds.value = fileList.value
      .map(file => Number(file.id))
      .filter(id => Number.isFinite(id) && id > 0);
    totalFileCount.value = fileList.value.length;
    totalFileSize.value = fileList.value.reduce((total, file) => total + Number(file.size || 0), 0);

    const groups = isFilmstripView.value ? [] : [
      textMatches.length > 0
        ? {
            id: 'search-text',
            label: localeMsg.value.search.text_matches,
            icon: IconFileSearch,
            files: textMatches,
            countLabel: String(textMatches.length),
          }
        : null,
      objectMatches.length > 0
        ? {
            id: 'search-object',
            label: localeMsg.value.search.object_matches,
            icon: IconFileSearch,
            files: objectMatches,
            countLabel: String(objectMatches.length),
          }
        : null,
      visualMatches.length > 0
        ? {
            id: 'search-visual',
            label: localeMsg.value.search.semantic_matches,
            icon: IconPhotoSearch,
            files: visualMatches,
            countLabel: String(visualMatches.length),
          }
        : null,
    ].filter(Boolean) as Array<{ id: string; label: string; icon: any; files: any[]; countLabel: string }>;

    let fileIndex = 0;
    for (const group of groups) {
      const rowIndex = groupedRows.value.length;
      const size = group.files.reduce((total, file) => total + Number(file.size || 0), 0);
      groupedRows.value.push({
        type: 'group',
        id: `group-row-${group.id}`,
        group_id: group.id,
        label: group.label,
        icon: group.icon,
        count: group.files.length,
        countLabel: group.countLabel,
        size,
      }).catch(error=>{if(currentQuerySource.value==='smart')smartQueryError.value=String(error);});
      groupedTimelineGroups.value.push({
        groupId: group.id,
        label: group.label,
        icon: group.icon,
        count: group.files.length,
        size,
        rowIndex,
      });
      groupMetaMap.set(group.id, { count: group.files.length, size });
      groupFileIdsCache.set(group.id, group.files.map(file => Number(file.id)));

      for (let groupFileIndex = 0; groupFileIndex < group.files.length; groupFileIndex++) {
        // Always reference the reactive object stored in fileList. Thumbnail
        // updates made through the raw search result do not trigger a rerender.
        const loadedFile = fileList.value[fileIndex];
        const id = Number(loadedFile?.id || 0);
        if (id > 0) fileIdToGroupId.set(id, group.id);
        groupedRows.value.push({
          type: 'item',
          id: `search-item-${group.id}-${id}`,
          group_id: group.id,
          file_index: fileIndex,
          file: loadedFile,
        });
        fileIndex++;
      }
    }

    groupedModeActive.value = groups.length > 0;
    totalRowCount.value = groupedRows.value.length;
    selectedItemIndex.value = fileList.value.length > 0 ? 0 : -1;
    scrollPosition.value = 0;
    markDedupSourceUpdated(requestId);
    openImageViewer(0, false, true);

    if (fileList.value.length > 0) {
      getFileListThumb(fileList.value);
    }
  } finally {
    if (requestId === currentContentRequestId) {
      isLoading.value = false;
      hasLoadedInitialResult.value = true;
      contentReady.value = true;
    }
  }
}

let mediaAiSearchRequest = '';
watch(() => [mediaAiSearch.active, libConfig._libraryId], () => {
  if (mediaAiSearchRequest) void invoke('media_ai_cancel_request', { requestId: mediaAiSearchRequest });
});
async function getMediaAiSearchFileList(query: string, requestId: number) {
  if (mediaAiSearchRequest) void invoke('media_ai_cancel_request', { requestId: mediaAiSearchRequest });
  const aiRequestId = crypto.randomUUID(); mediaAiSearchRequest = aiRequestId;
  const libraryId = libConfig._libraryId;
  currentQuerySource.value = 'search'; currentSmartQueryParams.value = null; currentCollectionId.value = null;
  currentSearchFileIds.value = []; timelineData.value = []; imageSearchError.value = false;
  imageSearchLanguageUnsupported.value = false; isLoading.value = true; mediaAiSearch.error = '';
  try {
    const hits = await invoke<MediaHit[]>('media_ai_search', { libraryId, query, source: mediaAiSearch.source, modelId: mediaAiSearch.modelId || null, albumId: mediaAiSearch.albumId, fileType: Number(config.search.fileType || 0), limit: 100, requestId: aiRequestId });
    if (requestId !== currentContentRequestId || libraryId !== libConfig._libraryId) return;
    const ranked = rankedMediaHits(hits), rows = await getFilesByIds(ranked.map(hit => hit.fileId));
    if (requestId !== currentContentRequestId || libraryId !== libConfig._libraryId) return;
    const byId = new Map((rows || []).map((file: any) => [Number(file.id), file]));
    clearSelectionForFileListUpdate(); resetGroupingState();
    fileList.value = preserveLoadedThumbnails(ranked.flatMap(hit => { const file: any = byId.get(hit.fileId); return file && passesAlbumFilters(file) ? [{ ...file, _mediaAiHit: hit }] : []; }));
    mediaAiSearch.hits = hits; currentSearchFileIds.value = fileList.value.map(file => Number(file.id));
    contentCountIsAuthoritative.value = true; totalFileCount.value = fileList.value.length;
    totalFileSize.value = fileList.value.reduce((sum, file) => sum + Number(file.size || 0), 0);
    markDedupSourceUpdated(requestId); restoreInitialSelectionIfNeeded(); openImageViewer(0, false, true);
    lastVisibleRange = { start: -1, end: -1 }; visibleRangeSeqId++;
  } catch (e) {
    if (requestId !== currentContentRequestId || libraryId !== libConfig._libraryId) return;
    mediaAiSearch.error = String(e); mediaAiSearch.hits = [];
    clearSelectionForFileListUpdate(); clearContentRows(); totalFileCount.value = 0; totalFileSize.value = 0;
  } finally {
    if (mediaAiSearchRequest === aiRequestId) mediaAiSearchRequest = '';
    if (requestId === currentContentRequestId) { isLoading.value = false; hasLoadedInitialResult.value = true; contentReady.value = true; }
  }
}

let contentUpdateSeq = 0;

async function updateContent(force = false, preserveMultiSelection = selectMode.value) {
  const updateSeq = ++contentUpdateSeq;
  ocrPreviewDeferred.value = false;
  const newIndex = config.main.sidebarIndex;
  const nextAlbumId = newIndex === SIDEBAR.ALBUM ? Number(libConfig.album.id || 0) : 0;
  const isCurrentAlbumIndexing =
    newIndex === SIDEBAR.ALBUM &&
    libConfig.activePane !== 'collection' &&
    !!libConfig.album.id &&
    libConfig.album.id > 0 &&
    Number(libConfig.index.albumQueue[0] || 0) === Number(libConfig.album.id);

  if (
    !force &&
    isScanStreamingMode.value &&
    isCurrentAlbumIndexing &&
    nextAlbumId > 0
  ) {
    enterScanStreamingMode(nextAlbumId);
    queueScanStreamingPull(nextAlbumId, Number(libConfig.index.discovered || 0));
    return;
  }

  // Reset temp view mode on any content update
  tempViewMode.value = 'none';
  showQuickView.value = false;
  isSlideShow.value = false;
  stopSlideShow();

  backupState.value = null;
  
  // Increment request ID to cancel any previous thumbnail generation and reset queue
  invalidateThumbnailRequests();
  const requestId = ++currentContentRequestId;
  // Leaving the folder view re-arms its sync, so coming back picks up changes that
  // leave the folder mtime untouched (e.g. OneDrive "Free up space").
  if (newIndex !== SIDEBAR.ALBUM || !libConfig.album.id || libConfig.album.selected) {
    lastSyncedAlbumFolder = '';
  }

  contentReady.value = false;
  contentCountIsAuthoritative.value = false;
  imageSearchError.value = false;
  imageSearchLanguageUnsupported.value = false;
  isCurrentFolderExcluded.value = false;
  isCurrentFolderMissing.value = false;

  if (preserveMultiSelection) {
    captureSelectionForFileListRefresh();
  } else {
    pendingSelectionRestore = null;
  }

  // Reset file list immediately to reflect UI change
  clearSelectionForFileListUpdate();
  clearContentRows();
  isLoading.value = true;

  await nextTick();
  if (updateSeq !== contentUpdateSeq) return; // superseded by a later updateContent call
  await waitForContentLoadingPaint();
  if (requestId !== currentContentRequestId) return;

  if (libConfig.activePane === 'collection') {
    const collectionId = Number(libConfig.collection.selectedId || 0);
    if (collectionId <= 0) {
      contentTitle.value = localeMsg.value.collection.title;
      showEmptyContent(requestId);
      return;
    }

    const collections = await listCollections();
    const collection = Array.isArray(collections)
      ? collections.find((item: any) => Number(item.id) === collectionId)
      : null;
    if (!collection) {
      libConfig.collection.selectedId = null;
      contentTitle.value = localeMsg.value.collection.title;
      showEmptyContent(requestId);
      return;
    }

    contentTitle.value = collection.name || localeMsg.value.collection.title;
    getCollectionFileList(collectionId, requestId);
    return;
  }

  if (newIndex === SIDEBAR.HOME && libConfig.activePane !== 'collection') {
    contentTitle.value = '';
    showEmptyContent(requestId);
    return;
  }

  if(newIndex === SIDEBAR.LIBRARY) {
    switch ((libConfig.library as any).item) {
      case LIB_ITEM.MEDIA: {
        const type=String(libConfig.library.smartId||'image');
        contentTitle.value=t('media_lifecycle.title')+' > '+t('media_lifecycle.types.'+type);
        getSmartFileList({query:{version:2,match:'all',rules:mediaTypeRules(type)},sort:{type:config.search.sortType,order:config.search.sortOrder},group:{type:config.search.groupBy}},requestId);
        break;
      }
      case LIB_ITEM.HIDDEN:
      case LIB_ITEM.DELETED:
        showEmptyContent(requestId);break;
      case LIB_ITEM.FAV:
        contentTitle.value = localeMsg.value.favorite.files;
        getFileList({ isFavorite: true }, requestId);
        break;
      case LIB_ITEM.TODAY:
        contentTitle.value = localeMsg.value.library.on_this_day;
        getFileList({ startDate: -1, endDate: -1, calendarSort: DATE_SORT.TAKEN_DESC }, requestId);
        break;
      case LIB_ITEM.RATINGS:
        if (libConfig.rating.item === RATE.ALL) {
          contentTitle.value = `${localeMsg.value.rating.title} > ${localeMsg.value.rating.rated}`;
          getFileList({ rating: RATE.ALL }, requestId);
        } else if (libConfig.rating.item === RATE.UNRATED) {
          contentTitle.value = `${localeMsg.value.rating.title} > ${localeMsg.value.rating.unrated}`;
          getFileList({ rating: RATE.UNRATED }, requestId);
        } else if ((libConfig.rating.item || 0) > 0) {
          const rating = Number(libConfig.rating.item || 0);
          const key = ratingLabelKey(rating);
          contentTitle.value = `${localeMsg.value.rating.title} > ${key ? localeMsg.value.rating[key] : `${rating}★`}`;
          getFileList({ rating }, requestId);
        } else {
          contentTitle.value = localeMsg.value.rating.title;
          showEmptyContent(requestId);
        }
        break;
      case LIB_ITEM.CULLING: {
        const cullingItem = libConfig.culling.item;
        const label = cullingItem === CULLING.PICK
          ? localeMsg.value.culling.picks
          : cullingItem === CULLING.REJECT
            ? localeMsg.value.culling.rejected
            : localeMsg.value.culling.unreviewed;
        contentTitle.value = `${localeMsg.value.culling.title} > ${label}`;
        const cullingFlag = cullingItem === CULLING.PICK
          ? 1
          : cullingItem === CULLING.REJECT
            ? 2
            : 0;
        getFileList({ cullingFlag }, requestId);
        break;
      }
      case LIB_ITEM.SUBJECTS: {
        const smartId = libConfig.library.smartId;
        if (!smartId) {
          contentTitle.value = localeMsg.value.subject.title;
          showEmptyContent(requestId);
          break;
        }
        const smartTag = getSmartTagById(smartId);
        if (!smartTag) {
          contentTitle.value = localeMsg.value.subject.title;
          showEmptyContent(requestId);
          break;
        }
        const smartTagLabel = localeMsg.value.subject.items?.[smartTag.id] || smartTag.id;
        contentTitle.value = `${localeMsg.value.subject.title} > ${smartTagLabel}`;
        getImageSearchFileList(
          smartTag.prompt,
          0,
          requestId,
          smartTag.threshold ?? config.smartTagSearchThreshold,
        );
        break;
      }
      default:
        contentTitle.value = localeMsg.value.library.all_files;
        getFileList({}, requestId);
        break;
    }
  }
  else if(newIndex === SIDEBAR.ALBUM) {
    if(libConfig.album.id === null) {
      contentTitle.value = "";
    } else if(libConfig.album.id === 0) {   // all files
      contentTitle.value = localeMsg.value.library.all_files;
      getFileList({}, requestId);
    } else {
      getAlbum(libConfig.album.id).then(async album => {
        if (requestId !== currentContentRequestId) return;
        if(album) {
          const wasInaccessible = album.is_accessible === false;
          album.is_accessible = await checkAlbumAccessibility(album.id);
          if (requestId !== currentContentRequestId) return;
          if (wasInaccessible && album.is_accessible !== false) {
            trustCachedThumbnailRequestId = currentThumbRequestId;
          }
          if(libConfig.album.selected) {     
            // album is selected, show all files including subfolders
            contentTitle.value = album.name;
            if (album.is_accessible === false) {
              if (requestId !== currentContentRequestId) return;
              isCurrentFolderMissing.value = true;
            }
            getFileList({ searchAllSubfolders: album.path }, requestId);
          } else {                        
            // folder is selected, show files in the folder
            const folderPath = libConfig.album.folderPath || "";
            const folderId = Number(libConfig.album.folderId || 0);
            const folderAccessible = album.is_accessible !== false && !!folderPath && await isDirectoryAccessible(folderPath);
            if (requestId !== currentContentRequestId) return;
            setFolderAccessibility(folderPath, folderAccessible);
            if (!folderAccessible) {
              if (requestId !== currentContentRequestId) return;
              isCurrentFolderMissing.value = true;
              contentTitle.value = formatFolderBreadcrumb(folderPath, album.path);
              getFileList(config.settings.showSubfolderFiles
                ? { searchAllSubfolders: folderPath }
                : { searchFolder: folderPath }, requestId);
              return;
            }
            getFolderSearchExcluded(folderPath).then(excluded => {
              if (requestId === currentContentRequestId) isCurrentFolderExcluded.value = !!excluded;
            });
            contentTitle.value = formatFolderBreadcrumb(folderPath, album.path);
            const folderNode = await fetchFolder(folderPath, false, config.settings.folderSort);
            if (requestId !== currentContentRequestId) return;
            selectedFolderHasChildren.value = Boolean(folderNode?.children?.length);
            const folderQueryParams = () => config.settings.showSubfolderFiles
              ? { searchAllSubfolders: folderPath }
              : { searchFolder: folderPath };
            const syncKey = `${album.id}:${folderId}:${folderPath}`;
            if (folderId > 0 && folderPath && lastSyncedAlbumFolder !== syncKey) {
              lastSyncedAlbumFolder = syncKey;
              // Debounce: if a sync is already in-flight for this folder, reuse it.
              const existing = pendingFolderSyncs.get(folderId);
              const syncPromise = existing ?? syncAlbumFolderMtimes(album.id, folderId, folderPath);
              if (!existing) pendingFolderSyncs.set(folderId, syncPromise);
              syncPromise.then(syncResult => {
                if (pendingFolderSyncs.get(folderId) === syncPromise) {
                  pendingFolderSyncs.delete(folderId);
                }
                if (
                  requestId !== currentContentRequestId ||
                  config.main.sidebarIndex !== SIDEBAR.ALBUM ||
                  libConfig.album.folderId !== folderId ||
                  libConfig.album.folderPath !== folderPath
                ) return;
                if (syncResult?.current_folder_synced) {
                  void refreshFolderSidebarCounts([folderPath]);
                  console.log(
                    `folder sync: ${syncResult.new_file_count} new, ${syncResult.updated_file_count} updated, ${syncResult.deleted_file_count} deleted, ${syncResult.rename_count || 0} renamed`
                  );
                  const visibleRange = { ...lastVisibleRange };
                  const refreshRequestId = ++currentContentRequestId;
                  if (pendingInitialSelectedIndex < 0) {
                    pendingInitialSelectedIndex = selectedItemIndex.value;
                    hasRestoredInitialSelection = false;
                  }
                  getFileList(folderQueryParams(), refreshRequestId).then(() => {
                    if (
                      refreshRequestId !== currentContentRequestId ||
                      visibleRange.start < 0 ||
                      visibleRange.end <= visibleRange.start
                    ) return;
                    fetchDataRange(visibleRange.start, visibleRange.end + 1);
                  });
                }
              }).catch(error => {
                if (pendingFolderSyncs.get(folderId) === syncPromise) {
                  pendingFolderSyncs.delete(folderId);
                }
                console.error('folder sync failed:', error);
              });
            }
            getFileList(folderQueryParams(), requestId);
          }
        } else {
          contentTitle.value = "";
        }
      });
    }
  }
  else if(newIndex === SIDEBAR.SMART_ALBUM) {
    const smartAlbumType = libConfig.smartAlbum.type;
    const smartAlbumId = libConfig.smartAlbum.id;
    if (smartAlbumType === 'custom' && smartAlbumId) {
      const smartAlbum = (libConfig.smartAlbums || []).find((item: any) => item.id === smartAlbumId);
      if (smartAlbum) {
        contentTitle.value = smartAlbum.name || '';
        getSmartFileList(smartAlbum, requestId);
      } else {
        contentTitle.value = "";
        showEmptyContent(requestId);
      }
    } else {
      contentTitle.value = "";
      showEmptyContent(requestId);
    }
  }
  else if(newIndex === SIDEBAR.SEARCH) {
    if (isStructuredSearchView.value) {
      contentTitle.value = t('structured_search.title');
      if (activeStructuredAlbum.value) getSmartFileList(activeStructuredAlbum.value, requestId);
      else getFileList({ searchFileType: 0 }, requestId);
    } else if (libConfig.search.searchText) {
      contentTitle.value = localeMsg.value.search.title + ' - ' + libConfig.search.searchText;
      getUnifiedSearchFileList(libConfig.search.searchText, requestId);
    } else {
      contentTitle.value = localeMsg.value.search.title;
      currentQuerySource.value = 'search';
      currentSmartQueryParams.value = null;
      currentCollectionId.value = null;
      currentSearchFileIds.value = [];
      showEmptyContent(requestId);
    }
  }
  else if(newIndex === SIDEBAR.CALENDAR) {
    if(libConfig.calendar.year === null) {
      contentTitle.value = "";
      showEmptyContent(requestId);
    } else {
      if (libConfig.calendar.month === -1) {          // yearly
        contentTitle.value = formatDate(libConfig.calendar.year!, 1, 1, 'year');
      } else if (libConfig.calendar.date === -1) {    // monthly
        contentTitle.value = formatDate(libConfig.calendar.year!, libConfig.calendar.month!, 1, 'month');
      } else {                                    // daily
        contentTitle.value = formatDate(libConfig.calendar.year!, libConfig.calendar.month!, libConfig.calendar.date!, 'date_long');
      }
      const [startDate, endDate] = getCalendarDateRange(libConfig.calendar.year!, libConfig.calendar.month!, libConfig.calendar.date!);
      getFileList({ startDate, endDate }, requestId);
    }
  } 
  else if(newIndex === SIDEBAR.TAG) {
    if (libConfig.tag.groupId) {
      const groupId = libConfig.tag.groupId;
      getTagGroupName(groupId).then(name => {
        if (requestId !== currentContentRequestId) return;
        if (name) {
          contentTitle.value = name;
          getFileList({ tagGroupId: groupId }, requestId);
        } else {
          contentTitle.value = "";
          showEmptyContent(requestId);
        }
      }).catch(() => { if (requestId === currentContentRequestId) showEmptyContent(requestId); });
    } else if (libConfig.tag.id === null) {
      contentTitle.value = "";
      showEmptyContent(requestId);
    } else {
      getTagName(libConfig.tag.id, true).then(tagName => {
        if (requestId !== currentContentRequestId) return;
        if (tagName) {
          contentTitle.value = tagName;
          getFileList({ tagId: libConfig.tag.id || 0 }, requestId);
        } else {
          contentTitle.value = "";
          showEmptyContent(requestId);
        }
      });
    }
  }
  else if(newIndex === SIDEBAR.OBJECTS) {
    contentTitle.value = `${t('objects.title')} · ${libConfig.objects.classKey ? objectClassName({key:libConfig.objects.classKey,name:libConfig.objects.className},t) + ' ≥ ' + libConfig.objects.minCount : t('objects.state_' + normalizeObjectState(libConfig.objects.state))}`;
    getFileList({ searchFileType: 0, objectState: normalizeObjectState(libConfig.objects.state), objectClass: libConfig.objects.classKey || '', objectMinCount: libConfig.objects.minCount || 1 }, requestId);
  }
  else if(newIndex === SIDEBAR.OCR) {
    contentTitle.value = `${t('ocr.workspace_title')} · ${t('ocr.state_' + normalizeOcrState(libConfig.ocr.state))}${libConfig.ocr.text ? ' — ' + libConfig.ocr.text : ''}`;
    getFileList({ ocrState: normalizeOcrState(libConfig.ocr.state), ocrText: libConfig.ocr.text || '' }, requestId);
  }
  else if(newIndex === SIDEBAR.PERSON) {
    if (libConfig.person.id === null) {
      contentTitle.value = "";
      showEmptyContent(requestId);
    } else {
      contentTitle.value = libConfig.person.name || `${localeMsg.value.sidebar.people}`;
      getFileList({ personId: libConfig.person.id }, requestId);
    }
  }
  else if(newIndex === SIDEBAR.LOCATION) {
    if (!libConfig.location.nodeId) {
      contentTitle.value = "";
      showEmptyContent(requestId);
    } else {
      contentTitle.value = displayLocationFilterLabel(libConfig.location);
      getFileList({locationNodeId: libConfig.location.nodeId}, requestId);
    }
  }
  else if(newIndex === SIDEBAR.CAMERA) {
    if (!config.camera.isCamera) {
      const lensMake = (libConfig.camera as any).lensMake;
      const lensModel = (libConfig.camera as any).lensModel;
      if (lensMake === null) {
        contentTitle.value = "";
        showEmptyContent(requestId);
      } else if (lensModel) {
        contentTitle.value = `${lensMake} > ${lensModel}`;
        getFileList({ lensMake, lensModel }, requestId);
      } else {
        contentTitle.value = `${lensMake}`;
        getFileList({ lensMake }, requestId);
      }
    } else if(libConfig.camera.make === null) {
      contentTitle.value = "";
      showEmptyContent(requestId);
    } else {
      if(libConfig.camera.model) {
        contentTitle.value = `${libConfig.camera.make} > ${libConfig.camera.model}`;
        getFileList({ make: libConfig.camera.make, model: libConfig.camera.model }, requestId);
      } else {
        contentTitle.value = `${libConfig.camera.make}`;
        getFileList({ make: libConfig.camera.make }, requestId);
      } 
    }
  } 

  if(fileList.value.length === 0) {
    isLoading.value = false;
  }
}

// --- Similar Search Mode Logic ---
function enterSimilarSearchMode(file: any) {
  if (file.file_type !== 1 && file.file_type !== 3) {
    return;
  }

  // Increment request ID to cancel any previous thumbnail generation and reset queue
  invalidateThumbnailRequests();
  // 1. Backup current state
  if (tempViewMode.value === 'none') {
    backupState.value = createViewBackup();
  }

  // 2. Set mode
  tempViewMode.value = 'similar';
  showQuickView.value = false;
  
  // 3. Update Title to indicate context
  contentTitle.value = localeMsg.value.search.similar_images + ' - ' + mediaDisplayName(file);

  // 4. Perform Search (reusing existing API call logic)
  const requestId = ++currentContentRequestId;
  showLoadingContent(requestId);
  
  // Reset scroll and selection
  scrollPosition.value = 0;
  selectedItemIndex.value = 0;
  if (gridViewRef.value) {
    gridViewRef.value.scrollToPosition(0);
  }
  
  getImageSearchFileList("", file.id, requestId);
}

// --- Person Search Mode Logic ---
async function enterPersonSearchMode(file: any) {
  if (!config.settings.face.enabled) {
    return;
  }
  if (!file || !file.id) {
    return;
  }

  // fetch faces
  const faces = await getFacesForFile(file.id);
  if (!faces || faces.length === 0) {
     toast.info(localeMsg.value.tooltip.not_found.recognized_person || "No recognized person found");
     return;
  }

  // Find first face with person_id
  const face = faces.find((f: any) => f.person_id && f.person_id > 0);
  if (!face) {
     toast.info(localeMsg.value.tooltip.not_found.recognized_person || "No recognized person found");
     return;
  }

  await enterPersonTempView(Number(face.person_id), face.person_name || '');
}

// Open a temporary view of all photos of a specific person. Used by both
// "find this person" and clicking a person name in the file info panel.
async function enterPersonTempView(personId: number, personName: string) {
  if (!config.settings.face.enabled || !personId || personId <= 0) return;

  // Increment request ID to cancel any previous thumbnail generation and reset queue
  invalidateThumbnailRequests();
  // 1. Backup current state
  if (tempViewMode.value === 'none') {
    backupState.value = createViewBackup();
  }

  // 2. Set mode
  tempViewMode.value = 'person';
  showQuickView.value = false;

  // 3. Update libConfig.person to reflect the selected person
  suppressPersonContextRefresh = true;
  libConfig.person.id = personId;
  libConfig.person.name = personName || null;
  await nextTick();
  suppressPersonContextRefresh = false;

  // 4. Update Title to indicate context
  contentTitle.value = personName || localeMsg.value.sidebar.people;

  // 5. Perform Search
  const requestId = ++currentContentRequestId;
  showLoadingContent(requestId);

  // Reset scroll and selection
  scrollPosition.value = 0;
  selectedItemIndex.value = 0;
  if (gridViewRef.value) {
    gridViewRef.value.scrollToPosition(0);
  }

  getFileList({ personId, searchFileType: 0 }, requestId);
}

// Click a person name in the file info panel → "find this person".
function handleNavigatePerson(payload: { personId: number; personName: string }) {
  void enterPersonTempView(Number(payload?.personId || 0), payload?.personName || '');
}

function enterAlbumPreviewMode(file: any, targetFolderPath?: string) {
  if (!file.album_id) return;
  const folderPath = targetFolderPath || getFolderPath(file.file_path);
  if (!folderPath) return;
  if (tempViewMode.value === 'album' && currentQueryParams.value.searchFolder === folderPath) return;

  // 1. Backup current state
  if (tempViewMode.value === 'none') {
    backupState.value = createViewBackup();
  }
  
  // Increment request ID to cancel any previous thumbnail generation and reset queue
  invalidateThumbnailRequests();
  // 2. Set mode
  tempViewMode.value = 'album';
  showQuickView.value = false;
  
  // 3. Update Title and Fetch Files
  getAlbum(file.album_id).then((album: any) => {
    if (album) {
      if(folderPath === album.path) { // current folder is root
        contentTitle.value = album.name;
      } else {
        contentTitle.value = formatFolderBreadcrumb(folderPath || "", album.path);
      };
    }
  });

  // 4. Fetch Files
  const requestId = ++currentContentRequestId;
  
  // Reset list for loading state
  clearContentRows();
  totalFileCount.value = 0;
  totalFileSize.value = 0;
  
  // Reset scroll and selection
  scrollPosition.value = 0;
  selectedItemIndex.value = 0;
  if (gridViewRef.value) {
    gridViewRef.value.scrollToPosition(0);
  }
  
  getFileList({ searchFolder: folderPath }, requestId);
}

function exitTempViewMode() {
  if (!backupState.value) return;

  const state = backupState.value;
  
  // 1. Restore State
  clearSelectionForFileListUpdate();
  for (const file of state.fileList) {
    if (file) file.isSelected = false;
  }
  // Keep thumbnails loaded in the temporary view when restoring its source list.
  fileList.value = preserveLoadedThumbnails(state.fileList);
  groupedModeActive.value = Boolean(state.groupedModeActive);
  groupedRows.value = state.groupedRows || [];
  totalRowCount.value = Number(state.totalRowCount || 0);
  groupedTimelineGroups.value = state.groupedTimelineGroups || [];
  folderGroupRoots.value = state.folderGroupRoots || [];
  groupMetaMap.clear();
  for (const [groupId, meta] of state.groupMetaEntries || []) {
    groupMetaMap.set(groupId, meta);
  }
  fileIdToGroupId.clear();
  for (const [fileId, groupId] of state.fileIdToGroupIdEntries || []) {
    fileIdToGroupId.set(fileId, groupId);
  }
  groupSelectedCountMap.value = {};
  groupSelectedSizeMap.value = {};
  totalFileCount.value = state.totalFileCount;
  totalFileSize.value = state.totalFileSize;
  contentTitle.value = state.contentTitle;
  selectedItemIndex.value = state.selectedItemIndex;
  scrollPosition.value = state.scrollPosition;
  timelineData.value = state.timelineData;
  currentQueryParams.value = state.currentQueryParams;
  currentQuerySource.value = state.currentQuerySource || 'query';
  currentSmartQueryParams.value = state.currentSmartQueryParams || null;
  currentCollectionId.value = state.currentCollectionId || null;
  currentSearchFileIds.value = [...(state.currentSearchFileIds || [])];
  currentMapSearchExactMembers = Boolean(state.currentMapSearchExactMembers);
  // Increment request ID to cancel any previous thumbnail generation (from temp view)
  invalidateThumbnailRequests();
  if (tempViewMode.value === 'map' && isRealFileItem(fileList.value[selectedItemIndex.value])) {
    // The restored selection can be outside the grid viewport; the old request
    // was cancelled on entering or leaving the temporary view.
    void getFileListThumb([fileList.value[selectedItemIndex.value]], { viewportBound: false });
  }

  // 2. Reset Mode
  tempViewMode.value = 'none';
  showQuickView.value = false;
  backupState.value = null;

  // 3. Restore Scroll Position (need to wait for Vue to re-render the list)
  // Using nextTick or a small timeout
  setTimeout(() => {
    if (gridViewRef.value) {
      gridViewRef.value.scrollToPosition(state.scrollTop);
    }
  }, 0);
  
  // 4. Ensure the originally selected item is highlighted/visible logic is handled by the restored index
  updateSelectedImage(selectedItemIndex.value);
}

function handleTitleClick() {
  switch (tempViewMode.value) {
    case 'similar':
    case 'camera':
    case 'lens':
    case 'location':
      exitTempViewMode();
      break;
    case 'person':
      config.main.sidebarIndex = SIDEBAR.PERSON;
      break;
    case 'album':
      config.main.sidebarIndex = SIDEBAR.ALBUM;

      // Get first file to extract album info
      const file = fileList.value[0];
      if (!file || !file.album_id || !file.folder_id) return;
      
      const folderPath = getFolderPath(file.file_path);
      
      // Set libConfig.album to select this folder in Album tab
      libConfig.album.id = file.album_id;
      libConfig.album.folderId = file.folder_id;
      libConfig.album.folderPath = folderPath;
      libConfig.album.selected = false;
      
      // Emit event to trigger album expansion in AlbumList
      tauriEmit('expand-album-folder', { albumId: file.album_id, folderPath: folderPath });
      break;
    default:
      break;
  }

  exitTempViewMode();
}

// update file state after a save from either FileInfo or EditImage
const onFileSaved = async (success: boolean, payload: SavedFilePayload = {}) => {
  if (success) {
    if (payload.saveAsNew && payload.filePath) {
      uiStore.updateFileVersion(payload.filePath);
      clearPreviewPreloadCache(payload.filePath);
      const inserted = await indexAndInsertSavedFile(payload.filePath, payload.saveAsContext || null);
      if (!inserted) {
        await updateContent();
      } else {
        getCurrentQueryTimeLine().then(data => {
          timelineData.value = data;
        }).catch(error=>{if(currentQuerySource.value==='smart')smartQueryError.value=String(error);});
      }
      toast.success(localeMsg.value.tooltip.save_image.save_as_success || localeMsg.value.tooltip.save_image.success);
    } else {
      const savedFile = fileList.value[selectedItemIndex.value];
      if (savedFile?.file_path) {
        uiStore.updateFileVersion(savedFile.file_path);
        clearPreviewPreloadCache(savedFile.file_path);
      }
      if (savedFile && Number(savedFile.rotate || 0) !== 0) {
        savedFile.rotate = 0;
        await setFileRotate(savedFile.id, 0);
        await syncFileMetaToImageViewer(savedFile.id, { rotate: 0 });
      }
      await updateFile(fileList.value[selectedItemIndex.value]);
      toast.success(localeMsg.value.tooltip.save_image.success);
    }
  } else {
    toast.error(localeMsg.value.tooltip.save_image.failed);
  }
}

const getClipboardFilePaths = (files: any[], limit = 10) => {
  const paths: string[] = [];
  const seenPaths = new Set<string>();

  for (const file of files) {
    const primaryPath = String(file?.file_path || '');
    if (!primaryPath) continue;

    const groupPaths = [primaryPath];
    if (file?.media_subtype === 'raw_jpeg_pair') {
      const companionPath = String(file.live_photo_video_path || '');
      if (companionPath) groupPaths.push(companionPath);
    }

    const uniqueGroupPaths = groupPaths.filter(path => !seenPaths.has(path));
    if (paths.length + uniqueGroupPaths.length > limit) continue;
    for (const path of uniqueGroupPaths) {
      seenPaths.add(path);
      paths.push(path);
    }
  }

  return paths;
};

const clickCopyImages = async (fallbackFile?: any) => {
  if (!await requireCurrentOriginals()) return;
  if (isProcessing.value) return;

  let copiedCount = 0;
  let requestedCount = 0;
  let cancelled = false;
  let filePaths: string[] = [];
  try {
    if (selectMode.value && selectedCount.value > 0) {
      requestedCount = selectedCount.value;
      const selectedItems = await getSelectedItemsForClipboard(10);
      if (!selectedItems) {
        cancelled = true;
        return;
      }
      filePaths = getClipboardFilePaths(selectedItems, 20);
    } else if (fallbackFile) {
      filePaths = getClipboardFilePaths([fallbackFile]);
      requestedCount = filePaths.length;
    }
    if (filePaths.length === 0) {
      cancelled = true;
      return;
    }
    isProcessing.value = true;
    copiedCount = Number(await copyImages(filePaths)) || 0;
  } finally {
    isProcessing.value = false;
    if (cancelled) {
      return;
    } else if (copiedCount > 0) {
      toast.success(t('tooltip.copy_image.success', { count: copiedCount.toLocaleString() }));
      if (requestedCount > 10) {
        toast.warning(t('tooltip.copy_image.limit', { count: 10 }));
      }
    } else {
      toast.error(localeMsg.value.tooltip.copy_image.failed);
    }
  }
}

const appPathForMediaKind = (kind: 'image' | 'video') =>
  String(config.defaultExternalApp(kind)?.path || '');

const launchInExternalApp = async (paths: string[], appPath: string) => {
  try {
    await openFilesWithApp(paths, appPath);
  } catch (error) {
    console.error('Failed to open external app:', error);
    toast.error(t('tooltip.open_external.failed'));
  }
};

const confirmExternalOpen = () => {
  const pending = pendingExternalOpen.value;
  showExternalOpenWarningMsgbox.value = false;
  pendingExternalOpen.value = null;
  if (pending) void launchInExternalApp(pending.paths, pending.appPath);
};

const cancelExternalOpen = () => {
  showExternalOpenWarningMsgbox.value = false;
  pendingExternalOpen.value = null;
};

const openInExternalApp = async (appId?: string) => {
  const items = selectMode.value
    ? (selectedCount.value > 0 ? await getActionableSelectedItemsForAction() : [])
    : [fileList.value[selectedItemIndex.value]].filter(Boolean);
  if (!items || items.length === 0 || !await requireOriginalFiles(items)) return;

  const kind = getMediaKind(items);
  if (kind === 'empty') return;
  if (kind === 'mixed') {
    toast.warning(t('tooltip.open_external.mixed_selection'));
    return;
  }

  const appPath = appId
    ? String(config.externalAppsFor(kind).find((app: any) => app.id === appId)?.path || '')
    : appPathForMediaKind(kind);
  if (!appPath) {
    toast.warning(t('tooltip.open_external.no_app'));
    return;
  }

  const paths = items.map((item: any) => item.file_path).filter(Boolean);
  if (paths.length === 0) return;

  if (paths.length > EXTERNAL_OPEN_WARNING_THRESHOLD) {
    pendingExternalOpen.value = { paths, appPath };
    showExternalOpenWarningMsgbox.value = true;
    return;
  }

  await launchInExternalApp(paths, appPath);
}

const onRenameFile = async (newName: string) => {
  if (!await requireCurrentOriginals()) return;
  if(selectedItemIndex.value >= 0) {
    const file = fileList.value[selectedItemIndex.value];
    const fileName = combineFileName(newName, renamingFileName.value.ext ?? '');
    if (fileName === file.name) {
      showRenameMsgbox.value = false;
      return;
    }
    let newFilePath;
    try {
      newFilePath = await renameFile(file.id, file.file_path, fileName);
    } catch (error) {
      errorMessage.value = `${localeMsg.value.msgbox.rename_file.error}: ${String(error)}`;
      return;
    }
    if(newFilePath) {
      console.log('onRenameFile:', newFilePath);
      file.name = fileName;
      file.file_path = newFilePath;
      showRenameMsgbox.value = false;
      errorMessage.value = '';
    } else {
      errorMessage.value = localeMsg.value.msgbox.rename_file.error;
    }
  }
}

const refreshAffectedAlbums = async (albumIds: Array<number | null | undefined>, refreshCounts = false) => {
  const libraryId = libConfig._libraryId;
  const uniqueAlbumIds = Array.from(
    new Set(
      albumIds
        .map((id) => Number(id || 0))
        .filter((id) => id > 0),
    ),
  );

  if (uniqueAlbumIds.length === 0) return;

  const albums = (
    await Promise.all(uniqueAlbumIds.map((albumId) => recountAlbum(albumId)))
  ).filter(Boolean);

  if (albums.length > 0 && libConfig._libraryId === libraryId) {
    await tauriEmit('albums-refreshed', { albums, refreshFolders: false, refreshCounts });
  }
};

const refreshLibraryTotalCount = async () => {
  await tauriEmit('library-total-refreshed', { source: 'content' });
};

function rebuildSelectionAfterListMutation(selectedIds: Set<number>) {
  let remainingCount = 0;
  let remainingSize = 0;
  selectedFileIds.clear();
  for (const file of fileList.value) {
    const selected = isRealFileItem(file) && selectedIds.has(Number(file.id));
    file.isSelected = selected;
    if (selected) {
      selectedFileIds.add(Number(file.id));
      remainingCount++;
      remainingSize += Number(file.size || 0);
    }
  }
  recomputeLoadedGroupSelectedCounts();
  selectedCount.value = remainingCount;
  selectedSize.value = remainingCount === totalFileCount.value ? totalFileSize.value : remainingSize;
  syncSelectionVersions();

  if (remainingCount === 0) {
    selectMode.value = false;
    lastSelectedIndex.value = -1;
    keyboardSelectionAnchorIndex.value = -1;
  }
}

const onMoveTo = async () => {
  if (!await requireCurrentOriginals()) return;
  const affectedAlbumIds = new Set<number>();
  const affectedFolderPaths = new Set<string>();
  const destPath = String(libConfig.destFolder.folderPath || '');
  const destAlbumId = Number(libConfig.destFolder.albumId || 0);
  const destLabel = getFolderName(destPath) || destPath;
  let successCount = 0;
  const failures: unknown[] = [];

  // Resolve destination folder ID: when the user selects an album root (not a
  // subfolder), destFolder.folderId is null. Ensure the root folder record
  // exists in afolders so that moved files keep a valid folder_id.
  let destFolderId = Number(libConfig.destFolder.folderId || 0);
  if (destFolderId <= 0 && destPath) {
    if (destAlbumId > 0) {
      const resolved = await selectFolder(destAlbumId, destPath);
      if (resolved?.id) {
        destFolderId = resolved.id;
      }
    }
  }

  if (selectMode.value && selectedCount.value > 0) {    // multi-select mode
    const items = await getActionableSelectedItemsForAction();
    if (!items) return;
    if (!await confirmLargeBatch(items.length)) return;
    const moves = await withFileTransferBatch(async (batchId: number) => runWithKeyedConcurrency(
      items,
      item => getTransferDestinationKey(item),
      async item => {
        const movedFile = await moveFile(item.id, item.file_path, destFolderId, destPath, 'keep_both', batchId);
        return { item, movedFile };
      },
    ));
    const successfulMoves = moves.flatMap(result =>
      result.status === 'fulfilled' && result.value.movedFile ? [result.value] : [],
    );
    const successIds = successfulMoves.map(({ item }) => item.id);
    successfulMoves.forEach(({ item, movedFile }) => {
      affectedFolderPaths.add(getFolderPath(item.file_path));
      console.log('onMoveTo:', movedFile);
      affectedAlbumIds.add(Number(item.album_id || 0));
    });
    for (const result of moves) {
      if (result.status === 'rejected') failures.push(result.reason);
      else if (!result.value.movedFile) failures.push(null);
    }
    if (successIds.length > 0) {
      successCount = successIds.length;
      const successIdSet = new Set(successIds);
      const remainingSelectedIds = new Set(
        items
          .filter(item => !successIdSet.has(item.id))
          .map(item => Number(item.id)),
      );
      fileList.value = fileList.value.filter((f) => !successIdSet.has(f.id));
      totalFileCount.value = fileList.value.length;
      totalFileSize.value = fileList.value.reduce((total, file) => total + file.size, 0);
      selectedItemIndex.value = fileList.value.length > 0 ? Math.min(selectedItemIndex.value, fileList.value.length - 1) : -1;
      rebuildSelectionAfterListMutation(remainingSelectedIds);
      toast.success(t('msgbox.move_to.success', {
        source: t('toolbar.filter.select_count', { count: successCount.toLocaleString() }), dest: destLabel,
      }));
    }
  } 
  else if(selectedItemIndex.value >= 0) {               // single select mode
    const file = fileList.value[selectedItemIndex.value];
    const sourceLabel = file.name;
    try {
      const movedFile = await moveFile(file.id, file.file_path, destFolderId, destPath);
      if(movedFile) {
        affectedFolderPaths.add(getFolderPath(file.file_path));
        console.log('onMoveTo:', movedFile);
        affectedAlbumIds.add(Number(file.album_id || 0));
        removeFromFileList(selectedItemIndex.value);
        successCount = 1;
        toast.success(t('msgbox.move_to.success', { source: sourceLabel, dest: destLabel }));
      } else failures.push(null);
    } catch (error) {
      failures.push(error);
    }
  }

  if (successCount > 0) {
    if (destAlbumId > 0) affectedAlbumIds.add(destAlbumId);
    await refreshFolderSidebarCounts([...affectedFolderPaths, destPath]);
    await refreshAffectedAlbums(Array.from(affectedAlbumIds));
    await refreshLibraryTotalCount();
    if (groupedModeActive.value || effectiveGroupBy.value > 0) await updateContent(true);
  }

  if (failures.length || successCount === 0) {
    const sourceLabel = selectMode.value
      ? t('toolbar.filter.select_count', { count: selectedCount.value.toLocaleString() })
      : (fileList.value[selectedItemIndex.value]?.name || '');
    toast.error(fileTransferFailure(t('msgbox.move_to.error', { source: sourceLabel, dest: destLabel }), failures));
  }

  if (!failures.length && successCount > 0) showMoveTo.value = false;
}

const selectSystemDestination = async (title: string) => {
  const destination = await openDialog({
    title,
    multiple: false,
    directory: true,
  });
  return !destination || Array.isArray(destination) ? null : String(destination);
}

const requestFileConflict = (
  name: string,
  destination: string,
  showApplyAll: boolean,
  allowReplace = true,
): Promise<{ policy: FileConflictPolicy; applyAll: boolean }> => {
  fileConflictDialog.value = { show: true, name, destination, showApplyAll, allowReplace };
  return new Promise(resolve => {
    fileConflictResolver = resolve;
  });
}

const resolveFileConflict = (result: { policy: FileConflictPolicy; applyAll: boolean }) => {
  fileConflictDialog.value.show = false;
  fileConflictResolver?.(result);
  fileConflictResolver = null;
}

const resolveConflictPolicy = async (
  sourcePath: string,
  name: string,
  destPath: string,
  applyAllPolicy: FileConflictPolicy | null,
  showApplyAll: boolean,
  sameDestinationMeansSkip: boolean,
) => {
  const destinationPath = getFullPath(destPath, name);
  const isSamePath =
    normalizePathForCompare(sourcePath) === normalizePathForCompare(destinationPath);
  if (isSamePath && sameDestinationMeansSkip) {
    toast.info(t('msgbox.file_conflict.same_folder'));
    return { policy: 'skip' as FileConflictPolicy, applyAll: false };
  }
  if (!await checkFileExists(destinationPath)) {
    return { policy: 'keep_both' as FileConflictPolicy, applyAll: false };
  }
  if (applyAllPolicy && !(isSamePath && applyAllPolicy === 'replace')) {
    return { policy: applyAllPolicy, applyAll: true };
  }
  return requestFileConflict(name, destPath, showApplyAll, !isSamePath);
}

const getFilesForFolderAction = async () => {
  if (selectMode.value) {
    return selectedCount.value > 0 ? await getActionableSelectedItemsForAction() : [];
  }
  return selectedItemIndex.value >= 0 ? [fileList.value[selectedItemIndex.value]] : [];
}

const getTransferDestinationKey = (file: any) =>
  String(file?.name || getFolderName(file?.file_path || '')).normalize('NFC').toLowerCase();

const resolveLibraryDestination = async (destPath: string) => {
  const albums = await getAllAlbums();
  const album = Array.isArray(albums)
    ? albums.find(item => isWithinRootPath(destPath, item.path))
    : null;
  if (!album) return null;

  const folder = await selectFolder(album.id, destPath);
  return folder?.id ? { albumId: Number(album.id), folderId: Number(folder.id) } : null;
}

const onMoveToFolder = async () => {
  if (!await requireCurrentOriginals()) return;
  const files = await getFilesForFolderAction();
  if (!files || files.length === 0) return;

  const destPath = await selectSystemDestination(t('msgbox.move_to_folder.title'));
  if (!destPath) return;
  if (!await confirmLargeBatch(files.length)) return;

  const sourceLabel = selectMode.value
    ? t('toolbar.filter.select_count', { count: files.length.toLocaleString() })
    : (files[0]?.name || '');
  const libraryDestination = await resolveLibraryDestination(destPath);
  if (!libraryDestination) {
    const confirmed = await ask(
      t('msgbox.move_to_folder.warning', { source: sourceLabel, dest: destPath }),
      {
        title: t('msgbox.move_to_folder.confirm_title'),
        kind: 'warning',
        okLabel: t('msgbox.move_to_folder.ok'),
        cancelLabel: t('msgbox.cancel'),
      },
    );
    if (!confirmed) return;
  }

  const affectedAlbumIds = new Set<number>();
  let applyAllPolicy: FileConflictPolicy | null = null;
  const operations: Array<{ file: any; policy: FileConflictPolicy }> = [];
  for (const file of files) {
    if (!file?.file_path) continue;
    const conflict = await resolveConflictPolicy(
      file.file_path,
      file.name,
      destPath,
      applyAllPolicy,
      files.length > 1,
      true,
    );
    if (conflict.applyAll) applyAllPolicy = conflict.policy;
    if (conflict.policy === 'skip') continue;
    operations.push({ file, policy: conflict.policy });
  }
  const moveResults = await withFileTransferBatch(async (batchId: number) => runWithKeyedConcurrency(
    operations,
    ({ file }) => getTransferDestinationKey(file),
    async ({ file, policy }) => {
      const movedFile = libraryDestination
        ? await moveFile(file.id, file.file_path, libraryDestination.folderId, destPath, policy, batchId)
        : await moveFileOutsideLibrary(file.id, file.file_path, destPath, policy, batchId);
      if (!movedFile) throw new Error(`Failed to move ${file.file_path}`);
      return { file, movedFile };
    },
  ));
  const successfulMoves = moveResults.flatMap(result =>
    result.status === 'fulfilled' ? [result.value] : [],
  );
  const successIds = successfulMoves.map(({ file }) => file.id);
  successfulMoves.forEach(({ file }) => {
    affectedAlbumIds.add(Number(file.album_id || 0));
  });
  if (libraryDestination && successfulMoves.length > 0) {
    affectedAlbumIds.add(libraryDestination.albumId);
  }

  if (successIds.length > 0) {
    const successIdSet = new Set(successIds);
    const remainingSelectedIds = new Set(
      files
        .filter(file => !successIdSet.has(file.id))
        .map(file => Number(file.id)),
    );
    fileList.value = fileList.value.filter(file => !successIdSet.has(file.id));
    totalFileCount.value = fileList.value.length;
    totalFileSize.value = fileList.value.reduce((total, file) => total + file.size, 0);
    selectedItemIndex.value = fileList.value.length > 0
      ? Math.min(selectedItemIndex.value, fileList.value.length - 1)
      : -1;
    rebuildSelectionAfterListMutation(remainingSelectedIds);
    toast.success(t('msgbox.move_to_folder.success', {
      source: files.length > 1 ? t('toolbar.filter.select_count', { count: successIds.length.toLocaleString() }) : sourceLabel,
      dest: getFolderName(destPath) || destPath,
    }));
    await refreshFolderSidebarCounts([
      ...successfulMoves.map(({ file }) => getFolderPath(file.file_path)),
      ...(libraryDestination ? [destPath] : []),
    ]);
    await refreshAffectedAlbums(Array.from(affectedAlbumIds));
    await refreshLibraryTotalCount();
    // Moving a file changes its grouping key (for example, folder grouping).
    // The flat list was updated optimistically above, but grouped rows and
    // group counts must be rebuilt from the current query result.
    if (groupedModeActive.value || effectiveGroupBy.value > 0) {
      await updateContent(true);
    }
  }
  const failedMoves = moveResults.filter(result => result.status === 'rejected');
  if (failedMoves.length > 0) {
    toast.error(fileTransferFailure(t('msgbox.move_to_folder.error', {
      source: files.length > 1 ? t('toolbar.filter.select_count', { count: failedMoves.length.toLocaleString() }) : sourceLabel,
      dest: getFolderName(destPath) || destPath,
    }), failedMoves.map(result => result.reason)));
  }
}

const onCopyToFolder = async () => {
  if (!await requireCurrentOriginals()) return;
  const destPath = await selectSystemDestination(t('msgbox.copy_to_folder.title'));
  if (!destPath) return;

  const files = await getFilesForFolderAction();
  if (!files || files.length === 0) return;
  if (!await confirmLargeBatch(files.length)) return;

  const libraryDestination = await resolveLibraryDestination(destPath);
  const destLabel = getFolderName(destPath) || destPath;
  const sourceLabel = selectMode.value
    ? t('toolbar.filter.select_count', { count: files.length.toLocaleString() })
    : (files[0]?.name || '');
  let applyAllPolicy: FileConflictPolicy | null = null;
  const operations: Array<{ file: any; policy: FileConflictPolicy }> = [];

  for (const file of files) {
    if (!file?.file_path) continue;
    const conflict = await resolveConflictPolicy(
      file.file_path,
      file.name,
      destPath,
      applyAllPolicy,
      files.length > 1,
      false,
    );
    if (conflict.applyAll) applyAllPolicy = conflict.policy;
    if (conflict.policy === 'skip') continue;
    operations.push({ file, policy: conflict.policy });
  }
  const copyResults = await withFileTransferBatch(async (batchId: number) => runWithKeyedConcurrency(
    operations,
    ({ file }) => getTransferDestinationKey(file),
    async ({ file, policy }) => {
      const copiedFile = await copyFile(file.file_path, destPath, policy, batchId);
      if (!copiedFile) throw new Error(`Failed to copy ${file.file_path}`);
      if (libraryDestination) {
        const indexedFile = await addFileToDb(libraryDestination.folderId, copiedFile);
        if (!indexedFile) {
          throw new CopyIndexError(`Copied but failed to index ${copiedFile}`);
        }
      }
      return { file, copiedFile };
    },
  ));
  const successCount = copyResults.filter(result => result.status === 'fulfilled').length;
  const indexFailureCount = copyResults.filter(
    result => result.status === 'rejected' && result.reason instanceof CopyIndexError,
  ).length;
  const copyFailureCount = copyResults.filter(
    result => result.status === 'rejected' && !(result.reason instanceof CopyIndexError),
  ).length;

  if (successCount > 0) {
    toast.success(t('msgbox.copy_to_folder.success', {
      source: files.length > 1 ? t('toolbar.filter.select_count', { count: successCount.toLocaleString() }) : sourceLabel,
      dest: destLabel,
    }));
    if (libraryDestination) {
      await refreshFolderSidebarCounts([destPath]);
      await refreshAffectedAlbums([libraryDestination.albumId]);
      await refreshLibraryTotalCount();
      await tauriEmit('refresh-content');
    }
  }
  if (copyFailureCount > 0) {
    toast.error(fileTransferFailure(t('msgbox.copy_to_folder.error', {
      source: files.length > 1 ? t('toolbar.filter.select_count', { count: copyFailureCount.toLocaleString() }) : sourceLabel,
      dest: destLabel,
    }),
      copyResults.flatMap(result => result.status === 'rejected' && !(result.reason instanceof CopyIndexError) ? [result.reason] : [])));
  }
  if (indexFailureCount > 0) {
    toast.error(t('msgbox.copy_to_folder.index_error', {
      count: indexFailureCount.toLocaleString(),
    }));
  }
}

async function handleDropUrls(
  urls: string[],
  resolvedDestination?: { albumId: number; folderId: number; folderPath: string },
) {
  const destination = resolvedDestination || await resolveCurrentAlbumImportDestination();
  if (!destination) return;
  const { albumId, folderId, folderPath } = destination;
  let imported = 0;
  for (const url of urls) {
    try {
      const file = await importUrl(url, folderId, folderPath);
      if (file) imported++;
    } catch (e) {
      console.error('Failed to import URL:', url, e);
    }
  }
  if (imported > 0) {
    await refreshImportedFiles(albumId, folderPath);
    toast.success(t('msgbox.drop_import.success', { count: imported }));
  } else {
    toast.warning(t('msgbox.drop_import.no_files'));
  }
}

const getDeleteFileSuccessMessage = (fileName: string, permanently: boolean) =>
  permanently
    ? localeMsg.value.msgbox.permanent_delete.file_success.replace('{file}', fileName)
    : localeMsg.value.msgbox.move_to_trash.file_success.replace('{file}', fileName);

const getDeleteFilesSuccessMessage = (count: number, permanently: boolean) =>
  permanently
    ? localeMsg.value.msgbox.permanent_delete.files_success.replace('{count}', count.toLocaleString())
    : localeMsg.value.msgbox.move_to_trash.files_success.replace('{count}', count.toLocaleString());

const getDeleteFileErrorMessage = (permanently: boolean) =>
  permanently
    ? localeMsg.value.msgbox.permanent_delete.file_error
    : localeMsg.value.msgbox.move_to_trash.file_error;

const getDeleteFilesErrorMessage = (permanently: boolean) =>
  permanently
    ? localeMsg.value.msgbox.permanent_delete.files_error
    : localeMsg.value.msgbox.move_to_trash.files_error;

const onTrashAllDuplicates = async () => {
  const libraryId = libConfig._libraryId;
  try {
    const permanently = deletePermanently.value;
    const result = await dedupDelete({ deleteAll: true, permanently });
    if (libConfig._libraryId !== libraryId) return;
    const deletedCount = Number(result?.deletedCount || 0);
    const failedCount = Number(result?.failedCount || 0);

    if (deletedCount > 0) {
      await refreshFolderSidebarCounts(result?.affectedFolderPaths || []);
      await updateContent(true);
      await dedupPaneRef.value?.refreshGroups();
      toast.success(getDeleteFilesSuccessMessage(deletedCount, permanently));
    }
    if (failedCount > 0) {
      toast.error(getDeleteFilesErrorMessage(permanently));
    }
    if (deletedCount === 0 && failedCount === 0) {
      toast.warning(localeMsg.value.info_panel.dedup.no_removable_files);
    }
  } catch (error) {
    console.error('Failed to delete all removable duplicates:', error);
    toast.error(getDeleteFilesErrorMessage(deletePermanently.value));
  } finally {
    closeTrashMsgbox();
  }
};

const handleTrashMsgboxOk = async () => {
  if (isTrashDeleting.value) return;
  isTrashDeleting.value = true;
  try {
    if (isDedupBulkTrash.value) {
      await onTrashAllDuplicates();
    } else {
      await onTrashFile();
    }
  } finally {
    isTrashDeleting.value = false;
  }
};

const onTrashFile = async (retryItemsOverride: any[] = []) => {
  permanentDeleteChecked.value = deletePermanently.value;
  const permanently = deletePermanently.value;
  const deletedFileIds: number[] = [];
  const affectedAlbumIds = new Set<number>();
  const affectedFolderPaths = new Set<string>();
  const recordDeletedFile = (file: any) => {
    affectedAlbumIds.add(Number(file.album_id || 0));
    affectedFolderPaths.add(getFolderPath(file.file_path));
  };
  let failedDeleteCount = 0;
  let otherFailureCount = 0;
  pendingTrashFailedItems.value = [];
  pendingTrashFailedDedupGroupKey.value = '';
  pendingTrashFailedOtherFailureCount.value = 0;
  const shouldUpdateDedup =
    isDedupPanelOpen.value &&
    !!dedupTrashGroupKey.value;
  const isSimilarDedupTrash = dedupTrashGroupKey.value.startsWith('similar:');
  const currentDedupGroupId = Number(
    isSimilarDedupTrash
      ? dedupTrashGroupKey.value.slice('similar:'.length)
      : dedupTrashGroupKey.value,
  );
  try {
    if (dedupDeleteFileIds.value.length > 0) {
      const ids = [...dedupDeleteFileIds.value];
      if (permanently || isSimilarDedupTrash) {
        const selectedItems = (await Promise.all(ids.map(async (id) =>
          fileList.value.find(file => Number(file?.id) === id) || getFileInfo(id)
        ))).filter((file): file is any => !!file?.id && !!file.file_path);
        const result = await batchDeleteFiles(
          selectedItems.map(item => ({ fileId: item.id, filePath: item.file_path })),
          permanently,
        );
        if (result?.jobId) return;
        if (!result) throw new Error(`Failed to ${permanently ? 'permanently delete' : 'trash'} dedup files`);
        const deletedIdSet = new Set(result.deletedFileIds.map((id: any) => Number(id)));
        const deletedItems = selectedItems.filter(item => deletedIdSet.has(Number(item.id)));
        failedDeleteCount = Number(result.failedCount || 0) + (ids.length - selectedItems.length);
        if (!permanently && isSimilarDedupTrash) {
          const trashFailedIdSet = new Set(
            Array.isArray(result.trashFailedFileIds)
              ? result.trashFailedFileIds.map((id: any) => Number(id)).filter((id: number) => id > 0)
              : [],
          );
          if (trashFailedIdSet.size > 0) {
            pendingTrashFailedDedupGroupKey.value = dedupTrashGroupKey.value;
            pendingTrashFailedItems.value = selectedItems.filter(item => trashFailedIdSet.has(Number(item.id)));
            otherFailureCount = Math.max(0, failedDeleteCount - trashFailedIdSet.size);
            pendingTrashFailedOtherFailureCount.value = otherFailureCount;
          }
        }

        if (failedDeleteCount > 0 && deletedItems.length === 0) {
          throw new Error(`Failed to ${permanently ? 'permanently delete' : 'trash'} dedup files`);
        }

        deletedItems.forEach(recordDeletedFile);
        deletedFileIds.push(...deletedItems.map(item => item.id));
      } else {
        const result = await dedupDelete({ fileIds: ids });
        if (result !== undefined) {
          for (const path of result?.affectedFolderPaths || []) affectedFolderPaths.add(path);
          const resultDeletedIds = Array.isArray(result?.deletedFileIds)
            ? result.deletedFileIds.map((id: any) => Number(id)).filter((id: number) => id > 0)
            : [];
          const trashFailedIdSet = new Set(
            Array.isArray(result?.trashFailedFileIds)
              ? result.trashFailedFileIds.map((id: any) => Number(id)).filter((id: number) => id > 0)
              : [],
          );
          deletedFileIds.push(...resultDeletedIds);
          const deletedIdSet = new Set(deletedFileIds);
          fileList.value
            .filter(file => deletedIdSet.has(Number(file?.id)))
            .forEach(recordDeletedFile);
          failedDeleteCount = Number(result?.failedCount || 0);
          if (trashFailedIdSet.size > 0) {
            pendingTrashFailedDedupGroupKey.value = dedupTrashGroupKey.value;
            pendingTrashFailedItems.value = getFileItemsByIds(ids).filter(item => trashFailedIdSet.has(Number(item.id)));
            otherFailureCount = Math.max(0, failedDeleteCount - trashFailedIdSet.size);
            pendingTrashFailedOtherFailureCount.value = otherFailureCount;
          }
          if (deletedFileIds.length === 0) {
            failedDeleteCount = Math.max(1, failedDeleteCount);
          }
        } else {
          throw new Error('Failed to trash dedup files');
        }
      }

      if (failedDeleteCount > 0 && deletedFileIds.length === 0 && pendingTrashFailedItems.value.length === 0) {
        throw new Error(`Failed to ${permanently ? 'permanently delete' : 'trash'} dedup files`);
      }

      const deletedIdSet = new Set(deletedFileIds);
      fileList.value = fileList.value.filter((f) => !deletedIdSet.has(Number(f?.id)));
      totalFileCount.value = fileList.value.length;
      totalFileSize.value = fileList.value.reduce((total, file) => total + Number(file?.size || 0), 0);
      selectedItemIndex.value = fileList.value.length > 0 ? Math.min(selectedItemIndex.value, fileList.value.length - 1) : -1;
    }
    else if (selectMode.value && selectedCount.value > 0) {     // multi-select mode
      const selectedItems = retryItemsOverride.length > 0
        ? retryItemsOverride
        : await getActionableSelectedItemsForAction();
      if (!selectedItems) return;
      const result = await batchDeleteFiles(
        selectedItems.map(item => ({ fileId: item.id, filePath: item.file_path })),
        permanently,
      );
      if (result?.jobId) return;
      if (!result) {
        throw new Error(`Failed to ${permanently ? 'permanently delete' : 'trash'} selected files`);
      }
      const deletedIdSet = new Set(result.deletedFileIds.map((id: any) => Number(id)));
      const deletedItems = selectedItems.filter(item => deletedIdSet.has(Number(item.id)));
      failedDeleteCount = Number(result.failedCount || 0);
      const trashFailedIdSet = new Set(
        Array.isArray(result.trashFailedFileIds)
          ? result.trashFailedFileIds.map((id: any) => Number(id))
          : [],
      );

      if (failedDeleteCount > 0 && deletedItems.length === 0 && trashFailedIdSet.size === 0) {
        throw new Error(`Failed to ${permanently ? 'permanently delete' : 'trash'} selected files`);
      }

      deletedItems.forEach(recordDeletedFile);
      deletedFileIds.push(...deletedItems.map(item => item.id));
      const remainingSelectedIds = new Set(
        selectedItems
          .filter(item => !deletedIdSet.has(Number(item.id)))
          .map(item => Number(item.id)),
      );
      fileList.value = fileList.value.filter((f) => !deletedIdSet.has(Number(f?.id)));
      totalFileCount.value = fileList.value.length;
      totalFileSize.value = fileList.value.reduce((total, file) => total + Number(file?.size || 0), 0);
      selectedItemIndex.value = fileList.value.length > 0 ? Math.min(selectedItemIndex.value, fileList.value.length - 1) : -1;
      rebuildSelectionAfterListMutation(remainingSelectedIds);
      if (!permanently && trashFailedIdSet.size > 0) {
        pendingTrashFailedItems.value = selectedItems.filter(item => trashFailedIdSet.has(Number(item.id)));
        otherFailureCount = Math.max(0, failedDeleteCount - trashFailedIdSet.size);
        pendingTrashFailedOtherFailureCount.value = otherFailureCount;
      }
    } 
    else if(selectedItemIndex.value >= 0) {               // single select mode
      const selectedFile = retryItemsOverride[0] || fileList.value[selectedItemIndex.value];
      const deletedFileName = selectedFile?.name || '';
      const result = await deleteFileAlways(selectedFile, permanently);
      if (result?.jobId) return;
      const deletedIdSet = new Set(result.deletedFileIds.map((id: any) => Number(id)));
      const trashFailedIdSet = new Set(
        Array.isArray(result.trashFailedFileIds)
          ? result.trashFailedFileIds.map((id: any) => Number(id))
          : [],
      );
      if (!permanently && trashFailedIdSet.has(Number(selectedFile.id))) {
        pendingTrashFailedItems.value = [selectedFile];
        throw new Error(`Failed to trash file: ${selectedFile.file_path}`);
      }
      if (!deletedIdSet.has(Number(selectedFile.id))) {
        throw new Error(`Failed to ${permanently ? 'permanently delete' : 'trash'} file: ${selectedFile.file_path}`);
      }
      recordDeletedFile(selectedFile);
      deletedFileIds.push(selectedFile.id);
      failedDeleteCount = Number(result.failedCount || 0);
      removeFromFileList(selectedItemIndex.value);
      if (failedDeleteCount === 0) {
        toast.success(getDeleteFileSuccessMessage(deletedFileName, permanently));
      }
    }

    await refreshGroupedRowsAfterDelete(deletedFileIds);
    await refreshFolderSidebarCounts([...affectedFolderPaths]);
    await refreshAffectedAlbums(Array.from(affectedAlbumIds));
    await refreshLibraryTotalCount();

    // Search results keep their relevance order and never use date groups.
    if (isSearchLikeView.value || tempViewMode.value === 'similar') {
      timelineData.value = [];
    } else {
      getCurrentQueryTimeLine().then(data => {
        timelineData.value = data;
      }).catch(error=>{if(currentQuerySource.value==='smart')smartQueryError.value=String(error);});
    }

    if (deletedFileIds.length > 0) {
      const compareFileCount = removeDeletedFilesFromImageViewerSession(deletedFileIds);
      tauriEmit('files-deleted', {
        source: 'content',
        fileIds: deletedFileIds,
        fileCount: fileList.value.length,
        compareFileCount,
        selectedIndex: selectedItemIndex.value,
      });
    }

    if ((selectMode.value || isDedupTrash.value) && deletedFileIds.length > 0) {
      toast.success(
        getDeleteFilesSuccessMessage(deletedFileIds.length, permanently)
      );
    }

    if (failedDeleteCount > 0) {
      if (!permanently && pendingTrashFailedItems.value.length > 0) {
        if (pendingTrashFailedOtherFailureCount.value > 0) {
          toast.error(
            (dedupDeleteFileIds.value.length > 0 || selectMode.value)
              ? getDeleteFilesErrorMessage(permanently)
              : getDeleteFileErrorMessage(permanently)
          );
        }
        showTrashFailedMsgbox.value = true;
      } else {
        toast.error(
          (dedupDeleteFileIds.value.length > 0 || selectMode.value)
            ? getDeleteFilesErrorMessage(permanently)
            : getDeleteFileErrorMessage(permanently)
        );
      }
    }

    closeTrashMsgbox();
    updateSelectedImage(selectedItemIndex.value);

    if (shouldUpdateDedup && deletedFileIds.length > 0) {
      if (isSimilarDedupTrash) {
        dedupPaneRef.value?.applyDeletedSimilarFiles(currentDedupGroupId, deletedFileIds);
      } else {
        dedupPaneRef.value?.applyDeletedFiles(currentDedupGroupId, deletedFileIds);
      }
    }
  } catch (error) {
    console.error(`Failed to ${permanently ? 'permanently delete' : 'trash'} file(s):`, error);
    if (String(error).startsWith('original_unavailable:')) {
      closeTrashMsgbox();
      toast.warning(t('offline.requires_original'));
      return;
    }
    if (!permanently && pendingTrashFailedItems.value.length > 0) {
      if (pendingTrashFailedOtherFailureCount.value > 0) {
        toast.error(
          (dedupDeleteFileIds.value.length > 0 || selectMode.value)
            ? getDeleteFilesErrorMessage(permanently)
            : getDeleteFileErrorMessage(permanently)
        );
      }
      showTrashMsgbox.value = false;
      showTrashFailedMsgbox.value = true;
      return;
    }
    toast.error(
      (dedupDeleteFileIds.value.length > 0 || selectMode.value)
        ? getDeleteFilesErrorMessage(permanently)
        : getDeleteFileErrorMessage(permanently)
    );
  }
}

// Remove a file from disk first. Keep the DB row if the filesystem operation fails.
async function deleteFileAlways(file: any, permanently = false) {
  const deletedFile = permanently
    ? await deleteFilePermanently(file.id, file.file_path)
    : await batchDeleteFiles([{fileId:file.id,filePath:file.file_path}], false);
  if (deletedFile?.jobId) return deletedFile;
  if (deletedFile && Array.isArray(deletedFile.deletedFileIds)) {
    console.log(`clickDeleteFile - ${permanently ? 'permanently deleted' : 'trashed'} file:`, file.file_path);
    return deletedFile;
  } else {
    throw new Error(`Failed to ${permanently ? 'permanently delete' : 'trash'} file: ${file.file_path}`);
  }
}

// remove an file item from the list and update the selected item index
function removeFromFileList(index: number = 0) {
  // remove file from list
  fileList.value.splice(index, 1);
  
  // update total file count and size
  totalFileCount.value = fileList.value.length;
  totalFileSize.value = fileList.value.reduce((total, file) => total + Number(file?.size || 0), 0);
  
  // update selected item index (ensure it's always a valid number)
  if (fileList.value.length > 0) {
    selectedItemIndex.value = Math.min(index, fileList.value.length - 1);
  } else {
    selectedItemIndex.value = -1;
  }
}

async function refreshGroupedRowsAfterDelete(fileIds: number[]) {
  if (!groupedModeActive.value || fileIds.length === 0) return;

  // Search builds its filename and visual-match groups client-side. The generic
  // grouping query deliberately excludes Search, so rebuild the unified result
  // after a deletion instead of resetting those groups.
  if (config.main.sidebarIndex === SIDEBAR.SEARCH && !isStructuredSearchView.value && libConfig.search.searchText) {
    await getUnifiedSearchFileList(libConfig.search.searchText, currentContentRequestId);
    return;
  }

  pendingRestoreScrollTop.value = gridViewRef.value?.getScrollTop() ?? null;
  await initializeGroupedFileList(currentContentRequestId);
}

async function refreshContentAfterFileInfoChange() {
  if (scanHasPendingEdits()) {
    metadataRefreshDeferred.value = { libraryId: libConfig._libraryId, requestId: currentContentRequestId };
    return;
  }
  metadataRefreshDeferred.value = null;
  rememberFocusedFileForPresentationRefresh();
  pendingRestoreScrollTop.value = gridViewRef.value?.getScrollTop() ?? null;
  if (tempViewMode.value === 'none') {
    await updateContent(true, true);
    return;
  }

  // Replay a temporary view's query without discarding its navigation backup.
  captureSelectionForFileListRefresh();
  contentReady.value = false;
  invalidateThumbnailRequests();
  const requestId = ++currentContentRequestId;
  if (tempViewMode.value === 'similar') {
    const { searchText, fileId, threshold } = currentImageSearchParams.value;
    await getImageSearchFileList(searchText, fileId, requestId, threshold);
  } else if (currentQuerySource.value === 'search') {
    await getMapSearchClusterFileList([...currentSearchFileIds.value], { ...currentQueryParams.value }, requestId, currentMapSearchExactMembers);
  } else {
    const sourceContext = currentQuerySource.value === 'smart'
      ? { source: 'smart' as const, smartParams: { ...currentSmartQueryParams.value } }
      : currentQuerySource.value === 'collection'
        ? { source: 'collection' as const, collectionId: currentCollectionId.value }
        : null;
    await getFileList({ ...currentQueryParams.value }, requestId, sourceContext, currentQueryParams.value.randomSeed);
  }
  if (requestId === currentContentRequestId) restoreScrollAfterRefresh();
}

function flushMetadataRefresh() {
  const scope = metadataRefreshDeferred.value;
  if (!scope) return;
  if (scope.libraryId !== libConfig._libraryId || scope.requestId !== currentContentRequestId) {
    metadataRefreshDeferred.value = null;
    return;
  }
  if (!scanHasPendingEdits()) void refreshContentAfterFileInfoChange();
}
watch([metadataRefreshDeferred, scanHasPendingEdits, () => libConfig._libraryId, fileList], flushMetadataRefresh);

// Refresh panel data without reloading whichever image is selected after the await.
const refreshFileInfoFromPanel = async (file: any) => {
  const libraryId = libConfig._libraryId;
  const fileId = Number(file.id);
  const requestId = currentContentRequestId;
  const result = await refreshFileInfo(libraryId, fileId);
  if (libraryId !== libConfig._libraryId || requestId !== currentContentRequestId || Number(file.id) !== fileId) return;
  if (!result?.file) throw new Error('File info refresh failed');
  const requiresQuery = metadataRefreshRequiresQuery(file, result.file, getGroupingQueryParams());
  Object.assign(file, result.file);
  if (!result.skipped) {
    clearCachedThumbnailDataUrl(file.id, config.settings.thumbnailSize);
    file.rawThumbnailStale = true;
  }
  if (result.error) toast.warning(result.error);
  fileInfoRevision.value++;
  void tauriEmit('tags-changed');
  if (requiresQuery) {
    await refreshContentAfterFileInfoChange();
  } else if (!result.skipped) {
    await getFileListThumb([file], { bustCache: true, viewportBound: false });
  }
};

// update the file info from the file; isRefresh = "Refresh file info" menu action
const updateFile = async (file: any, isRefresh = false) => {
  const libraryId = libConfig._libraryId;
  const fileId = Number(file.id);
  const requestId = currentContentRequestId;
  if (!await requireOriginalFiles([file])) return;
  if (isRefresh && (libraryId !== libConfig._libraryId || requestId !== currentContentRequestId)) return;
  try {
    // The refresh command leaves an unchanged cloud-only file unread, so skip
    // the forced thumbnail and image reload that would download it.
    const refreshed = isRefresh ? await refreshFileInfo(libraryId, fileId) : null;
    if (isRefresh && (libraryId !== libConfig._libraryId || requestId !== currentContentRequestId || Number(file.id) !== fileId)) return;
    const updatedFile = isRefresh ? refreshed?.file : await updateFileInfo(file.id, file.file_path);
    if (updatedFile) {
      const requiresQuery = isRefresh && metadataRefreshRequiresQuery(file, updatedFile, getGroupingQueryParams());
      Object.assign(file, updatedFile);
      if (isRefresh) { fileInfoRevision.value++; void tauriEmit('tags-changed'); }
      if (!refreshed?.skipped) {
        await updateThumbForFile(file);
        if (isRefresh && (libraryId !== libConfig._libraryId || requestId !== currentContentRequestId)) return;
        if (!requiresQuery) {
          if (isRefresh && (Number(fileList.value[selectedItemIndex.value]?.id) !== fileId || scanHasPendingEdits())) return;
          await updateSelectedImage(selectedItemIndex.value);
          if (isRefresh && (libraryId !== libConfig._libraryId || requestId !== currentContentRequestId
            || Number(fileList.value[selectedItemIndex.value]?.id) !== fileId || scanHasPendingEdits())) return;

          // Force Image.vue to reload the saved image by briefly nullifying file_path
          // to trigger its filePath watcher (since the path itself hasn't changed, only the version)
          const savedPath = file.file_path;
          file.file_path = '';
          await nextTick();
          file.file_path = savedPath;
          if (isRefresh && (libraryId !== libConfig._libraryId || requestId !== currentContentRequestId
            || Number(fileList.value[selectedItemIndex.value]?.id) !== fileId || scanHasPendingEdits())) return;

          // Clear CSS filter adjustments after image reload is triggered
          uiStore.clearActiveAdjustments();
        }
      }
      if (requiresQuery) {
        await refreshContentAfterFileInfoChange();
      }

      if (isRefresh) {
        if (refreshed?.error) toast.warning(refreshed.error);
        else toast.success(localeMsg.value.tooltip.update_image.success);
      }
    } else if (isRefresh) {
      toast.error(localeMsg.value.tooltip.update_image.failed);
    }
  } catch (err) {
    console.error('Failed to update file info:', err);
    if (isRefresh) {
      toast.error(localeMsg.value.tooltip.update_image.failed);
    }
  }
}

// force-update the thumbnail for the file
const VIDEO_THUMB_REFRESH_PERCENTS = [50, 90, 10];
const videoThumbRefreshIndex = new Map<number, number>();
const VIDEO_THUMB_REFRESH_MAP_MAX = 1000;

function getNextVideoThumbnailRefreshPercent(file: any) {
  if (file?.file_type !== 2 || !file?.id) return null;
  const fileId = Number(file.id);
  const currentIndex = videoThumbRefreshIndex.get(fileId) ?? 0;
  const percent = VIDEO_THUMB_REFRESH_PERCENTS[currentIndex % VIDEO_THUMB_REFRESH_PERCENTS.length];
  videoThumbRefreshIndex.set(fileId, (currentIndex + 1) % VIDEO_THUMB_REFRESH_PERCENTS.length);
  // Evict oldest entries when the map grows too large
  if (videoThumbRefreshIndex.size > VIDEO_THUMB_REFRESH_MAP_MAX) {
    const first = videoThumbRefreshIndex.keys().next().value;
    if (first !== undefined) videoThumbRefreshIndex.delete(first);
  }
  return percent;
}

const updateThumbForFile = async (file: any) => {
  const libraryId = libConfig._libraryId;
  const requestId = currentContentRequestId;
  const thumbnailSize = config.settings.thumbnailSize;
  file.rawThumbnailStale = true;
  clearCachedThumbnailDataUrl(file.id, thumbnailSize);
  const thumb = await getFileThumb(
    file.id,
    file.file_path,
    file.file_type,
    file.e_orientation || 0,
    thumbnailSize,
    true,
    getNextVideoThumbnailRefreshPercent(file),
    file.album_id || 0,
  );
  if (libraryId !== libConfig._libraryId || requestId !== currentContentRequestId) return;
  if (thumb) {
    if (thumb.error_code === 0 || thumb.error_code === 2) {
      file.thumbnail = getThumbnailDataUrl(thumb, thumbnailPlaceholder, true, thumbnailSize, file.file_path, Number(file.modified_at || 0), !file.cloud_only);
      file.rawThumbnailStale = false;
    } else if (thumb.error_code === 1) {
      file.rawThumbnailStale = false;
      file.thumbnail = thumbnailPlaceholder;
    }
  }
}

const syncFileMetaToImageViewer = async (fileId: number, changes: Record<string, any>) => {
  if (!fileId || !changes || Object.keys(changes).length === 0) return;
  tauriEmit('message-from-content', {
    message: 'update-file-meta',
    fileId,
    changes,
  });
};

function handleDedupCullingStatusUpdated(fileId: number, cullingFlag: number) {
  const file = fileList.value.find(item => Number(item.id) === Number(fileId));
  if (file) file.culling_flag = cullingFlag;
  void syncFileMetaToImageViewer(fileId, { culling_flag: cullingFlag });
  void tauriEmit('culling-status-updated', { fileIds: [fileId], cullingFlag });
}

type SavedFilePayload = {
  saveAsNew?: boolean;
  filePath?: string;
  saveAsContext?: SaveAsContext | null;
};

const insertIndexedFileIntoList = async (indexedFile: any) => {
  if (groupedModeActive.value) return false;
  const position = await getCurrentQueryFilePosition(indexedFile.id);
  if (position === null || position < 0) {
    return false;
  }

  const nextFile = {
    ...indexedFile,
    isSelected: false,
    rotate: indexedFile.rotate || 0,
  };

  const existingIndex = fileList.value.findIndex((file: any) => {
    const sameId = Number(file?.id || 0) === Number(indexedFile.id || 0);
    const samePath = file?.file_path && indexedFile.file_path && file.file_path === indexedFile.file_path;
    return sameId || samePath;
  });

  if (existingIndex >= 0) {
    const previousSize = Number(fileList.value[existingIndex]?.size || 0);
    fileList.value.splice(existingIndex, 1, nextFile);
    totalFileSize.value += Number(nextFile.size || 0) - previousSize;
    selectedItemIndex.value = existingIndex;
    markDedupSourceUpdated(currentContentRequestId);

    await nextTick();
    const updatedFile = fileList.value[existingIndex];
    if (!updatedFile) {
      return false;
    }
    await updateThumbForFile(updatedFile);
    updateSelectedImage(existingIndex);
    openImageViewer(existingIndex, false, false);
    return true;
  }

  totalFileCount.value += 1;
  totalFileSize.value += nextFile.size || 0;
  fileList.value.splice(position, 0, nextFile);
  selectedItemIndex.value = position;
  markDedupSourceUpdated(currentContentRequestId);

  await nextTick();
  const insertedFile = fileList.value[position];
  if (!insertedFile) {
    return false;
  }
  await updateThumbForFile(insertedFile);
  updateSelectedImage(position);
  openImageViewer(position, false, false);
  return true;
};

const indexAndInsertSavedFile = async (filePath: string, saveAsContext: SaveAsContext | null = null) => {
  const currentFile = fileList.value[selectedItemIndex.value];
  const folderId = Number(saveAsContext?.folderId || currentFile?.folder_id || 0);
  if (folderId <= 0) return false;

  const indexedFile = await addFileToDb(folderId, filePath);
  if (!indexedFile) return false;

  const fileId = Number(indexedFile.id || 0);
  if (fileId > 0 && saveAsContext?.collectionId) {
    try {
      await addFilesToCollection(saveAsContext.collectionId, [fileId]);
    } catch (error) {
      console.error('Failed to add saved file to collection:', error);
    }
  } else if (fileId > 0 && saveAsContext?.tagId) {
    const result = await addTagToFile(fileId, saveAsContext.tagId);
    if (result === null) {
      console.error('Failed to add saved file tag:', saveAsContext.tagId);
    }
  }

  return insertIndexedFileIntoList(indexedFile);
};

function advanceAfterMetadataShortcut(fileId: number, requestId: number) {
  if (!selectMode.value && requestId === currentContentRequestId
      && fileList.value[selectedItemIndex.value]?.id === fileId) {
    performNavigate('next');
  }
}

// toggle the selected file's favorite status (selectMode = false)
const toggleFavorite = async (advance = false) => {
  const requestId = currentContentRequestId;
  if (selectedItemIndex.value >= 0) {
    const item = fileList.value[selectedItemIndex.value];
    const previous = item.is_favorite;
    item.is_favorite = !previous;
    // update the favorite status in the database
    if (await setFileFavorite(item.id, item.is_favorite) === null) {
      item.is_favorite = previous;
      return;
    }
    syncFileMetaToImageViewer(item.id, { is_favorite: item.is_favorite });
    if (advance) advanceAfterMetadataShortcut(item.id, requestId);
  }
};

// set selected files' favorite status (selectMode = true)
const selectModeSetFavorites = async (isFavorite: boolean) => {
  if (selectMode.value && selectedCount.value > 0) {
    const items = await getActionableSelectedItemsForAction();
    if (!items) return;
    if (!await confirmLargeBatch(items.length)) return;
    const result = await batchUpdateFileMetadata({
      fileIds: items.map(item => item.id),
      isFavorite,
    });
    if (result === null) return;
    items.forEach(item => {
      item.is_favorite = isFavorite;
    });
    const activeItem = fileList.value[selectedItemIndex.value];
    if (activeItem?.isSelected) {
      syncFileMetaToImageViewer(activeItem.id, { is_favorite: isFavorite });
    }
  }
}

const toggleSelectModeFavorite = async () => {
  if (!selectMode.value || selectedCount.value === 0) return;
  const selectedItems = await getActionableSelectedItemsForAction();
  if (!selectedItems || selectedItems.length === 0) return;
  const shouldFavorite = !selectedItems.every(item => Boolean(item.is_favorite));
  await selectModeSetFavorites(shouldFavorite);
};

const setSelectedFileRating = async (rating: number, advance = false) => {
  const requestId = currentContentRequestId;
  if (selectedItemIndex.value >= 0) {
    const item = fileList.value[selectedItemIndex.value];
    const previous = item.rating;
    const normalized = item.rating === rating ? 0 : rating;
    item.rating = normalized;
    if (await setFileRating(item.id, normalized) === null) {
      item.rating = previous;
      return;
    }
    syncFileMetaToImageViewer(item.id, { rating: normalized });
    if (advance) advanceAfterMetadataShortcut(item.id, requestId);
  }
};

const selectModeSetRatings = async (rating: number) => {
  if (selectMode.value && selectedCount.value > 0) {
    const normalized = Math.max(0, Math.min(5, rating));
    const items = await getActionableSelectedItemsForAction();
    if (!items) return;
    if (!await confirmLargeBatch(items.length)) return;
    const result = await batchUpdateFileMetadata({
      fileIds: items.map(item => item.id),
      rating: normalized,
    });
    if (result === null) return;
    items.forEach(item => {
      item.rating = normalized;
    });
    const activeItem = fileList.value[selectedItemIndex.value];
    if (activeItem?.isSelected) {
      syncFileMetaToImageViewer(activeItem.id, { rating: normalized });
    }
  }
}

const setSelectedFileCullingFlag = async (cullingFlag: number, advance = false) => {
  const requestId = currentContentRequestId;
  if (selectedItemIndex.value < 0) return;
  const item = fileList.value[selectedItemIndex.value];
  if (!item) return;
  const normalized = Math.max(0, Math.min(2, cullingFlag));
  const previous = Number(item.culling_flag ?? item.cullingFlag ?? 0);
  item.culling_flag = normalized;
  const result = await setFileCullingFlag(item.id, normalized);
  if (result === null) {
    item.culling_flag = previous;
    return;
  }
  syncFileMetaToImageViewer(item.id, { culling_flag: normalized });
  void tauriEmit('culling-status-updated', { fileIds: [item.id], cullingFlag: normalized });
  if (advance) advanceAfterMetadataShortcut(item.id, requestId);
};

const selectModeSetCullingFlags = async (cullingFlag: number) => {
  if (!selectMode.value || selectedCount.value === 0) return;
  const items = await getActionableSelectedItemsForAction();
  if (!items) return;
  if (!await confirmLargeBatch(items.length)) return;
  const normalized = Math.max(0, Math.min(2, cullingFlag));
  const result = await batchUpdateFileMetadata({
    fileIds: items.map(item => item.id),
    cullingFlag: normalized,
  });
  if (result === null) return;
  items.forEach(item => {
    item.culling_flag = normalized;
  });
  const activeItem = fileList.value[selectedItemIndex.value];
  if (activeItem?.isSelected) {
    syncFileMetaToImageViewer(activeItem.id, { culling_flag: normalized });
  }
  void tauriEmit('culling-status-updated', { fileIds: items.map(item => item.id), cullingFlag: normalized });
};

// slide show
let slideShowIntervalId: NodeJS.Timeout | null = null;

function toggleSlideShow() {
  isSlideShow.value = !isSlideShow.value;
  if (isSlideShow.value) {
    startSlideShow();
  } else {
    stopSlideShow();
  }
}

function clearSlideShowTimer() {
  if (slideShowIntervalId) {
    clearInterval(slideShowIntervalId);
    slideShowIntervalId = null;
  }
}

// Check if current file is a video
function isCurrentFileVideo() {
  const currentFile = fileList.value[selectedItemIndex.value];
  return currentFile?.file_type === 2;
}

// Advance to next slide (handles looping)
function advanceSlideShow() {
  if (fileList.value.length === 0) return;
  
  if (selectedItemIndex.value >= fileList.value.length - 1) {
    selectedItemIndex.value = 0; // Loop
  } else {
    selectedItemIndex.value++;
  }
  
  // Schedule next advance based on file type
  scheduleNextSlide();
}

// Schedule the next slide transition
function scheduleNextSlide() {
  clearSlideShowTimer();
  
  if (!isSlideShow.value) return;
  
  // If current file is video, don't set timer - video's ended event will trigger next
  if (isCurrentFileVideo() && !isOriginalUnavailable(fileList.value[selectedItemIndex.value])) {
    return;
  }
  
  // For images, use the configured interval
  const interval = getSlideShowInterval(config.settings.slideShowInterval) * 1000;
  slideShowIntervalId = setTimeout(() => {
    advanceSlideShow();
  }, interval);
}

watch(() => isOriginalUnavailable(fileList.value[selectedItemIndex.value]), () => {
  if (isSlideShow.value) scheduleNextSlide();
});

function startSlideShow() {
  scheduleNextSlide();
}

function stopSlideShow() {
  isSlideShow.value = false;
  clearSlideShowTimer();
}

// Called when video ends in slideshow mode
function handleSlideshowNext() {
  if (isSlideShow.value) {
    advanceSlideShow();
  }
}

watch(() => config.settings.slideShowInterval, () => {
  if (isSlideShow.value && !isCurrentFileVideo()) {
    scheduleNextSlide();
  }
});

// set file rotate
const clickRotate = () => rotateSelected(90);

const rotateSelected = async (rotateDelta: number) => {
  if (selectMode.value && selectedCount.value === 0) return;
  if (selectMode.value && selectedCount.value > 0) {
    const items = await getActionableSelectedItemsForAction();
    if (!items) return;
    if (!await confirmLargeBatch(items.length)) return;
    const result = await batchUpdateFileMetadata({
      fileIds: items.map(item => item.id),
      rotateDelta,
    });
    if (result === null) return;
    items.forEach(item => {
      item.rotate = (Number(item.rotate) || 0) + rotateDelta;
    });
    const activeItem = fileList.value[selectedItemIndex.value];
    if (activeItem?.isSelected) {
      tauriEmit('message-from-content', { message: 'rotate', fileId: activeItem.id, rotateDelta });
      syncFileMetaToImageViewer(activeItem.id, { rotate: activeItem.rotate });
    }
    return;
  }

  if (selectedItemIndex.value >= 0) {
    fileList.value[selectedItemIndex.value].rotate = (Number(fileList.value[selectedItemIndex.value].rotate) || 0) + rotateDelta;

    // notify the image viewer
    tauriEmit('message-from-content', { message: 'rotate', fileId: fileList.value[selectedItemIndex.value].id, rotateDelta });

    // update the rotate status in the database
    setFileRotate(fileList.value[selectedItemIndex.value].id, fileList.value[selectedItemIndex.value].rotate);
    syncFileMetaToImageViewer(fileList.value[selectedItemIndex.value].id, {
      rotate: fileList.value[selectedItemIndex.value].rotate,
    });
  }
};

// set file tag
const clickTag = async () => {
  console.log('clickTag');
  if (selectMode.value && selectedCount.value === 0) return;
  if (selectMode.value) {
    const items = await getActionableSelectedItemsForAction();
    if (!items) return;
    const fileIds = items.map(file => file.id);
    if (!await confirmLargeBatch(fileIds.length)) return;
    fileIdsToTag.value = fileIds;
  } else if (selectedItemIndex.value >= 0) {
    fileIdsToTag.value = [fileList.value[selectedItemIndex.value].id];
  } else {
    fileIdsToTag.value = [];
  }
  showTaggingDialog.value = true;
}

const clickAddToCollection = async () => {
  if (selectMode.value && selectedCount.value === 0) return;
  if (selectMode.value) {
    const items = await getActionableSelectedItemsForAction();
    if (!items) return;
    if (!await confirmLargeBatch(items.length)) return;
    fileIdsToAddToCollection.value = items.map(file => Number(file.id)).filter(id => id > 0);
  } else if (selectedItemIndex.value >= 0) {
    fileIdsToAddToCollection.value = [Number(fileList.value[selectedItemIndex.value]?.id || 0)].filter(id => id > 0);
  } else {
    fileIdsToAddToCollection.value = [];
  }
  showAddToCollectionDialog.value = fileIdsToAddToCollection.value.length > 0;
};

const onEditComment = async (newComment: any) => {
  if (selectMode.value && selectedCount.value > 0) {
    const items = await getActionableSelectedItemsForAction();
    if (!items) return;
    if (!await confirmLargeBatch(items.length)) return;
    const result = await batchUpdateFileMetadata({
      fileIds: items.map(item => item.id),
      comment: newComment,
    });
    if (result === null) return;
    items.forEach(item => {
      item.comments = newComment;
    });
    const activeItem = fileList.value[selectedItemIndex.value];
    if (activeItem?.isSelected) {
      syncFileMetaToImageViewer(activeItem.id, { comments: newComment });
    }
    showCommentMsgbox.value = false;
    return;
  }

  if (selectedItemIndex.value >= 0) {
    const file = fileList.value[selectedItemIndex.value];
    const result = await editFileComment(file.id, newComment);
    if(result) {
      console.log('onEditComment:', newComment);
      file.comments = newComment;
      showCommentMsgbox.value = false;
      syncFileMetaToImageViewer(file.id, { comments: newComment });
    }
  }
}

const openCommentEditor = () => {
  if ((selectMode.value && selectedCount.value > 0) || (!selectMode.value && selectedItemIndex.value >= 0)) {
    showCommentMsgbox.value = true;
  }
}

const selectAllInCurrentList = async () => {
  boxSelection = null;
  if (groupedModeActive.value) {
    const nextCounts: Record<string, number> = {};
    const nextSizes: Record<string, number> = {};
    const ids = await getCurrentQueryFileIds();
    if (!Array.isArray(ids)) {
      toast.error(t('info_panel.selection_load_failed'));
      return;
    }

    selectedFileIds.clear();
    for (const id of Array.isArray(ids) ? ids : []) {
      const fileId = Number(id);
      if (Number.isFinite(fileId) && fileId > 0) selectedFileIds.add(fileId);
    }

    for (const groupId of groupMetaMap.keys()) {
      const meta = groupMetaMap.get(groupId);
      nextCounts[groupId] = Number(meta?.count || 0);
      nextSizes[groupId] = Number(meta?.size || 0);
    }

    for (const file of fileList.value) {
      if (isRealFileItem(file)) file.isSelected = true;
    }
    groupSelectedCountMap.value = nextCounts;
    groupSelectedSizeMap.value = nextSizes;
    selectedCount.value = selectedFileIds.size;
    selectedSize.value = selectedFileIds.size === totalFileCount.value ? totalFileSize.value : Object.values(nextSizes)
      .reduce((total, size) => total + Number(size || 0), 0);
    syncSelectionVersions();
    selectMode.value = true;
    if (isGridLayoutView.value && config.rightPanel.show) selectionPanelOpen.value = true;
    return;
  }

  const hasPlaceholders = fileList.value.some(isPendingFileItem);
  selectedFileIds.clear();
  if (hasPlaceholders) {
    const ids = await getCurrentQueryFileIds();
    if (!Array.isArray(ids)) {
      toast.error(t('info_panel.selection_load_failed'));
      return;
    }
    for (const id of ids) {
      const fileId = Number(id);
      if (Number.isFinite(fileId) && fileId > 0) selectedFileIds.add(fileId);
    }
  }

  let nextSize = 0;
  for (const file of fileList.value) {
    if (!file) continue;
    if (isRealFileItem(file)) {
      file.isSelected = true;
      selectedFileIds.add(Number(file.id));
      nextSize += Number(file.size || 0);
    } else {
      file.isSelected = false;
    }
  }
  recomputeLoadedGroupSelectedCounts();
  selectedCount.value = selectedFileIds.size;
  selectedSize.value = selectedFileIds.size === totalFileCount.value ? totalFileSize.value : nextSize;
  syncSelectionVersions();
  selectMode.value = true;
  if (isGridLayoutView.value && config.rightPanel.show) selectionPanelOpen.value = true;
};

const selectNoneInCurrentList = () => {
  boxSelection = null;
  clearLoadedSelectionFlags();
  resetSelectionSummary();
  groupSelectedCountMap.value = {};
  groupSelectedSizeMap.value = {};
  selectMode.value = true;
};

const invertSelectionInCurrentList = async () => {
  boxSelection = null;
  selectedFileIds.clear();
  let nextCount = 0;
  let nextSize = 0;
  for (let i = 0; i < fileList.value.length; i++) {
    const file = fileList.value[i];
    if (!file) continue;
    if (!isRealFileItem(file)) {
      file.isSelected = false;
      continue;
    }
    file.isSelected = !file.isSelected;
    if (file.isSelected) {
      selectedFileIds.add(Number(file.id));
      nextCount++;
      nextSize += Number(file.size || 0);
    }
  }
  recomputeLoadedGroupSelectedCounts();
  selectedCount.value = nextCount;
  selectedSize.value = nextCount === totalFileCount.value ? totalFileSize.value : nextSize;
  syncSelectionVersions();
  selectMode.value = true;
};

const handleSelectMode = (value: any) => {
  if (isScanStreamingMode.value) return;
  if (!value) boxSelection = null;
  selectMode.value = value;
  if(!selectMode.value) {
    clearLoadedSelectionFlags();
    resetSelectionSummary();
    groupSelectedCountMap.value = {};
    groupSelectedSizeMap.value = {};
  } else {
    showQuickView.value = false;
    stopSlideShow();
  }
};

function toggleSelectMode() {
  if (isScanStreamingMode.value || isMapVisible.value) return;
  checkUnsavedChanges(() => {
    const open = !selectionPanelOpen.value;
    if (selectionPanelOpen.value) config.rightPanel.show = false;
    handleSelectMode(open);
    selectionPanelOpen.value = open;
  });
}

function openSelectionPanel() {
  if (isScanStreamingMode.value || isMapVisible.value) return;
  checkUnsavedChanges(() => {
    if (!selectMode.value) handleSelectMode(true);
    selectionPanelOpen.value = true;
  });
}

function handleInfoAction(action: () => void) {
  if (selectMode.value) handleSelectMode(false);
  action();
}

watch(isMapView, (active) => {
  if (!active) return;
  stopSlideShow();
  if (selectMode.value) handleSelectMode(false);
});

const handleInfoNavigateFolder = (folderPath: string) => {
  const targetFile = fileList.value[selectedItemIndex.value];
  if (!folderPath || !targetFile?.album_id) return;
  enterAlbumPreviewMode(targetFile, folderPath);
};

async function handleViewerInfoNavigation(request: { libraryId: string; fileId: number; target: string; payload: any }) {
  const library = libConfig._libraryId;
  if (request.libraryId !== library || !['folder', 'metadata', 'person'].includes(request.target) || !(request.fileId > 0)) return;
  const file = await getFileInfo(request.fileId);
  if (!file || library !== libConfig._libraryId) return;
  checkUnsavedChanges(async () => {
    if (library !== libConfig._libraryId) return;
    if (request.target === 'folder' && typeof request.payload === 'string') enterAlbumPreviewMode(file, request.payload);
    else if (request.target === 'metadata') handleNavigateMetadata(request.payload);
    else if (request.target === 'person') handleNavigatePerson(request.payload);
    const mainWindow = await WebviewWindow.getByLabel('main');
    await mainWindow?.show();
    await mainWindow?.setFocus();
  });
}

const FILE_TYPE_IMAGE = 1;
const FILE_TYPE_VIDEO = 2;
const FILE_TYPE_RAW = 4;
const FILE_TYPE_ALL_MASK = FILE_TYPE_IMAGE | FILE_TYPE_VIDEO | FILE_TYPE_RAW;

function hasNonLatinLetters(text: string): boolean {
  return Array.from(text).some(char => /\p{L}/u.test(char) && !/\p{Script=Latin}/u.test(char));
}

function normalizeFileTypeMask(mask: number): number {
  if (!Number.isFinite(mask) || mask <= 0) return 0;
  const normalized = mask & FILE_TYPE_ALL_MASK;
  return normalized === 0 || normalized === FILE_TYPE_ALL_MASK ? 0 : normalized;
}

const emptyFilesMessage = computed(() => {
  if (config.main.sidebarIndex === SIDEBAR.OBJECTS && libConfig.activePane !== 'collection') return t('objects.no_results');
  if (config.main.sidebarIndex === SIDEBAR.OCR && libConfig.activePane !== 'collection') return t('ocr.no_results');
  if (isCurrentFolderMissing.value) return localeMsg.value.album.folder_unavailable.title;
  if (imageSearchError.value) return localeMsg.value.tooltip.not_found.image_search_failed;
  if (imageSearchLanguageUnsupported.value) return localeMsg.value.tooltip.not_found.image_search_language_unsupported;
  // if (currentQuerySource.value === 'collection') {
  //   return `${localeMsg.value.collection.empty_content}`;
  // }
  const notFound = localeMsg.value.tooltip.not_found;
  const mask = normalizeFileTypeMask(Number(config.search.fileType || 0));
  const messageKey = {
    [FILE_TYPE_IMAGE]: 'image_files',
    [FILE_TYPE_VIDEO]: 'video_files',
    [FILE_TYPE_RAW]: 'raw_files',
    [FILE_TYPE_IMAGE | FILE_TYPE_VIDEO]: 'image_video_files',
    [FILE_TYPE_IMAGE | FILE_TYPE_RAW]: 'image_raw_files',
    [FILE_TYPE_VIDEO | FILE_TYPE_RAW]: 'raw_video_files',
  }[mask];

  return (messageKey && notFound[messageKey]) || notFound.files;
});

const emptyFilesHint = computed(() => {
  if (isStructuredSearchView.value) return t('structured_search.empty_hint');
  if (config.main.sidebarIndex === SIDEBAR.OBJECTS && libConfig.activePane !== 'collection') return t('objects.no_results_hint');
  if (config.main.sidebarIndex === SIDEBAR.OCR && libConfig.activePane !== 'collection') return t('ocr.no_results_hint');
  if (isCurrentFolderMissing.value) return localeMsg.value.album.folder_unavailable.description;
  if (imageSearchError.value) return localeMsg.value.tooltip.not_found.image_search_failed_hint;
  if (imageSearchLanguageUnsupported.value) return localeMsg.value.tooltip.not_found.image_search_language_unsupported_hint;
  if (!showFolderFiles.value) return '';
  const notFound = localeMsg.value.tooltip.not_found;
  if (isCurrentFolderExcluded.value) return notFound.folder_excluded_hint || '';
  if (!config.settings.showSubfolderFiles) return notFound.folder_files_hint || '';
  return '';
});

const statusBarEmptyMessage = computed(() => (
  contentReady.value
  && !isLoading.value
  && !showWelcomeContent.value
  && fileList.value.length === 0
  && totalFileCount.value === 0
    ? localeMsg.value.tooltip.not_found.files
    : ''
));

const smartAlbumFileTypeMask = computed(() => {
  if (!isSmartAlbumView.value) return 0;
  const album = getActiveCustomSmartAlbum();
  if (!album) return 0;
  const ftRules = (album.query?.rules || []).filter((r: any) => r.field === 'file_type' && r.operator === 'is');
  return ftRules.reduce((mask: number, r: any) => mask | Number(r.value || 0), 0);
});

const activeFileTypeMask = computed(() =>
  isFixedAiResultView.value
    ? 0
    : isSmartAlbumView.value
    ? smartAlbumFileTypeMask.value
    : Number(config.search.fileType || 0)
);

const fileTypeSelectedValues = computed(() => {
  const mask = normalizeFileTypeMask(activeFileTypeMask.value);
  if (mask === 0) return [0];
  return [FILE_TYPE_IMAGE, FILE_TYPE_RAW, FILE_TYPE_VIDEO].filter(value => (mask & value) === value);
});

const fileTypeSummaryLabel = computed(() => {
  const options = fileTypeOptions.value;
  const mask = normalizeFileTypeMask(activeFileTypeMask.value);
  if (mask === 0) return options[0]?.label || '';

  const labels = [FILE_TYPE_IMAGE, FILE_TYPE_RAW, FILE_TYPE_VIDEO]
    .filter(value => (mask & value) === value)
    .map(value => options.find(option => option.value === value)?.label)
    .filter(Boolean);

  return labels.length > 0 ? labels.join(' + ') : (options[0]?.label || '');
});

const handleFileTypeSelect = (values: any[]) => {
  if (isScanStreamingMode.value) return;
  const nextValues = (Array.isArray(values) ? values : []).map(value => Number(value));
  const hasAll = nextValues.includes(0);
  const mask = hasAll ? 0 : nextValues.reduce((acc, value) => acc | value, 0);
  if (normalizeFileTypeMask(activeFileTypeMask.value) === normalizeFileTypeMask(mask)) return;
  rememberFocusedFileForPresentationRefresh();
  if (isSmartAlbumView.value) {
    const album = getActiveCustomSmartAlbum();
    if (!album) return;
    album.query.rules = (album.query.rules || []).filter((r: any) => r.field !== 'file_type');
    if (mask !== 0) {
      album.query.rules.push({
        id: crypto.randomUUID?.() || `${Date.now()}-${Math.random()}`,
        field: 'file_type',
        operator: 'is',
        value: mask,
      });
    }
    return;
  }
  config.search.fileType = normalizeFileTypeMask(mask);
};

const handleSortTypeSelect = (option: any, extendOption: any) => {
  if (isScanStreamingMode.value) return;
  rememberFocusedFileForPresentationRefresh();
  if (isSmartAlbumView.value) {
    const album = getActiveCustomSmartAlbum();
    if (album) {
      album.sort.type = option;
      album.sort.order = extendOption;
    }
  } else {
    config.search.sortType = option;
    config.search.sortOrder = extendOption;
  }
};

const handleGroupSelect = (optionIndex: any) => {
  if (isScanStreamingMode.value) return;
  rememberFocusedFileForPresentationRefresh();
  const nextGroupBy = Number(groupOptions.value[Number(optionIndex)]?.value ?? GROUP.NONE);
  if (isSmartAlbumView.value) {
    const album = getActiveCustomSmartAlbum();
    if (album) album.group = { ...(album.group || {}), type: nextGroupBy };
  } else {
    config.search.groupBy = nextGroupBy;
  }
};

const toggleInfoPanel = () => {
  checkUnsavedChanges(async () => {
    const preview = getActivePreviewMediaRef();
    const wasPreviewFullScreen = !!preview?.isFullScreen;
    if (wasPreviewFullScreen) {
      await preview.exitPreviewFullScreen();
      // A pending transition or failed window restore must not open a hidden panel.
      if (preview.isFullScreen) return;
    }
    const wasInfoPanelOpen = isInfoPanelOpen.value;
    if (selectMode.value) handleSelectMode(false);
    if (wasInfoPanelOpen && !wasPreviewFullScreen) {
      config.rightPanel.show = false;
      return;
    }
    selectionPanelOpen.value = false;
    config.rightPanel.mode = 'info';
    config.rightPanel.show = true;
  });
};

const toggleDedupPanel = () => {
  if (isScanStreamingMode.value) return;
  checkUnsavedChanges(() => {
    const wasDedupPanelOpen = isDedupPanelOpen.value;
    if (selectMode.value) handleSelectMode(false);
    if (wasDedupPanelOpen) {
      config.rightPanel.show = false;
      return;
    }
    void openDedupPanel();
  });
};

async function openDedupPanel() {
  const wasInSelectMode = selectMode.value;
  if (wasInSelectMode) selectMode.value = false;
  disablePreviewModes();
  config.rightPanel.mode = 'dedup';
  config.rightPanel.show = true;

  await nextTick();
  await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));

  if (!wasInSelectMode) return;
  clearLoadedSelectionFlags();
  resetSelectionSummary();
  groupSelectedCountMap.value = {};
  groupSelectedSizeMap.value = {};
}

let dedupNavigationRequestId = 0;

async function resolveGroupedFileIndex(fileId: number): Promise<number | null> {
  if (currentQuerySource.value === 'query') {
    const position = await getGroupedFilePosition(getGroupingQueryParams(), fileId);
    if (position === undefined) return null;
    return Number.isInteger(position) && Number(position) >= 0 ? Number(position) : -1;
  }

  let fileIndexCursor = 0;
  for (const group of groupedTimelineGroups.value) {
    const groupId = String(group.groupId || '');
    const groupCount = Number(group.count || 0);
    if (!groupId || groupCount <= 0) {
      fileIndexCursor += Math.max(0, groupCount);
      continue;
    }
    const ids = await getCachedGroupFileIds(groupId);
    if (!Array.isArray(ids)) return null;
    const groupFileIndex = ids.findIndex((id: number) => Number(id) === Number(fileId));
    if (groupFileIndex >= 0) return fileIndexCursor + groupFileIndex;
    fileIndexCursor += groupCount;
  }
  return -1;
}

async function resolveFileIndexForDedup(fileId: number): Promise<number> {
  const loadedIndex = fileList.value.findIndex(file => Number(file?.id) === Number(fileId));
  if (loadedIndex !== -1) return loadedIndex;

  const position = groupedModeActive.value
    ? await resolveGroupedFileIndex(fileId)
    : await getCurrentQueryFilePosition(fileId);
  if (position === null || position < 0 || position >= totalFileCount.value) {
    return -1;
  }

  const buffer = 200;
  if (groupedModeActive.value) {
    const rowIndex = getGroupedRowIndexForFileIndex(position);
    if (rowIndex < 0) return -1;
    await fetchGroupedRowsRange(rowIndex - buffer, rowIndex + buffer);
    if (!ensureGroupedFileAtIndex(position)) return -1;
  } else {
    await fetchDataRange(position - buffer, position + buffer);
  }

  return isRealFileItem(fileList.value[position]) ? position : -1;
}

const handleDedupSelectFile = (fileId: number) => {
  const requestId = ++dedupNavigationRequestId;
  checkUnsavedChanges(async () => {
    const index = await resolveFileIndexForDedup(fileId);
    if (requestId !== dedupNavigationRequestId) return;
    if (index === -1) return;
    selectedItemIndex.value = index;
    updateSelectedImage(index);
  });
};

const handleDedupPreviewFile = (fileId: number) => {
  const requestId = ++dedupNavigationRequestId;
  checkUnsavedChanges(async () => {
    const index = await resolveFileIndexForDedup(fileId);
    if (requestId !== dedupNavigationRequestId) return;
    if (index === -1) return;
    selectedItemIndex.value = index;
    handleItemDblClicked(index);
  });
};

const handleDedupTrashSelectedDuplicates = (groupKey: string, fileIds: number[], reclaimableBytes: number) => {
  if (!groupKey || !fileIds || fileIds.length === 0) return;
  openTrashMsgbox(reclaimableBytes, groupKey, fileIds);
};

const handleDedupTrashAllDuplicates = (fileCount: number, reclaimableBytes: number) => {
  openAllDuplicatesTrashMsgbox(fileCount, reclaimableBytes);
};

const handleDedupTrashSelectedSimilar = (groupKey: string, fileIds: number[], reclaimableBytes: number) => {
  if (!groupKey || !fileIds || fileIds.length === 0) return;
  openTrashMsgbox(reclaimableBytes, `similar:${groupKey}`, fileIds);
};

const handleDedupCompareSelectedPhotos = (files: any[]) => {
  const photos = files.filter(file => file?.file_type === 1 || file?.file_type === 3);
  if (photos.length < 2) return;
  void openImageViewer(0, true, false, {
    files: photos,
    forceSplitCount: photos.length === 2 ? 2 : 4,
  });
};

// file type options
const fileTypeOptions = computed(() => {
  const options = localeMsg.value.toolbar.filter?.file_type_options || [];
  return [
    { label: options[0] || 'All', value: 0 },
    { label: options[1] || 'Image', value: FILE_TYPE_IMAGE },
    { label: options[2] || 'RAW', value: FILE_TYPE_RAW },
    { label: options[3] || 'Video', value: FILE_TYPE_VIDEO },
  ];
});

// sort type options
const sortOptions = computed(() => {
  return mediaViewSortOptions(mediaSortOptions(getSelectOptions(localeMsg.value.toolbar.filter?.sort_type_options), t('media_title.filename'), t('media_title.sort')), t('media_views.count'), t('media_views.last_viewed'));
});

const fixedAiSortOptions = computed(() => [{
  label: localeMsg.value.toolbar.filter?.similarity || 'Similarity',
  value: 0,
}]);

const toolbarSortOptions = computed(() =>
  isSimilaritySortedResultView.value ? fixedAiSortOptions.value : sortOptions.value
);

// sort extend options
const sortExtendOptions = computed(() => {
  return getSelectOptions(localeMsg.value.toolbar.filter?.sort_order_options);
});

const toolbarSortExtendOptions = computed(() =>
  isSimilaritySortedResultView.value ? [] : sortExtendOptions.value
);

const groupTypeLabels = computed(() =>
  localeMsg.value.toolbar.filter?.group_type_options || []
);

const groupOptions = computed(() => {
  const options = [{
    label: groupTypeLabels.value[GROUP.NONE] || 'None',
    value: GROUP.NONE,
  }];

  return [
    ...options,
    ...[GROUP.FOLDER, GROUP.FILE_TYPE, GROUP.DAY, GROUP.MONTH, GROUP.YEAR, GROUP.RATING, GROUP.CULLING, GROUP.LOCATION, GROUP.CAMERA, GROUP.LENS]
      .map(value => ({ label: groupTypeLabels.value[value], value }))
      .filter(option => option.label),
  ];
});

const fixedAiGroupOptions = computed(() => [{
  label: isSubjectGroupingView.value || tempViewMode.value === 'similar' || tempViewMode.value === 'person'
    ? (groupTypeLabels.value[GROUP.NONE] || 'None')
    : (localeMsg.value.search.matches || 'Matches'),
  value: GROUP.NONE,
}]);

const toolbarGroupOptions = computed(() =>
  isFixedAiResultView.value ? fixedAiGroupOptions.value : groupOptions.value
);

const toolbarGroupIndex = computed(() => {
  if (isFixedAiResultView.value) return 0;
  const value = effectiveGroupBy.value;
  return Math.max(0, toolbarGroupOptions.value.findIndex(option => Number(option.value) === value));
});

const toolbarSortType = computed(() => {
  if (isSimilaritySortedResultView.value) return 0;
  return isSmartAlbumView.value
    ? Number(getActiveCustomSmartAlbum()?.sort?.type ?? 0)
    : Number(config.search.sortType || 0);
});

const toolbarSortOrder = computed(() => (
  isSmartAlbumView.value
    ? Number(getActiveCustomSmartAlbum()?.sort?.order ?? 1)
    : Number(config.search.sortOrder || 0)
));

const isSearchLikeView = computed(() => {
  return !isCollectionPane.value && (
    config.main.sidebarIndex === SIDEBAR.SMART_ALBUM ||
    config.main.sidebarIndex === SIDEBAR.SEARCH ||
    (config.main.sidebarIndex === SIDEBAR.LIBRARY && libConfig.library.item === LIB_ITEM.SUBJECTS)
  );
});

const isAiSearchMode = computed(() =>
  (config.main.sidebarIndex === SIDEBAR.SEARCH && !isStructuredSearchView.value) ||
  (config.main.sidebarIndex === SIDEBAR.LIBRARY && libConfig.library.item === LIB_ITEM.SUBJECTS)
);

const isFixedAiResultView = computed(() =>
  tempViewMode.value === 'person' ||
  isSimilaritySortedResultView.value
);

const isSimilaritySortedResultView = computed(() =>
  tempViewMode.value === 'similar' ||
  (libConfig.activePane !== 'collection' && isAiSearchMode.value)
);

const isSortControlDisabled = computed(() =>
  isFixedAiResultView.value ||
  tempViewMode.value === 'album' ||
  isScanStreamingMode.value
);

const isGroupControlDisabled = computed(() =>
  isFixedAiResultView.value ||
  isRandomSort.value ||
  !isGroupingControlAvailable.value ||
  isScanStreamingMode.value
);

const isSmartAlbumView = computed(() =>
  !isCollectionPane.value && config.main.sidebarIndex === SIDEBAR.SMART_ALBUM
);

const isSmartAlbumSortOverride = computed(() => {
  if (!isSmartAlbumView.value) return false;
  const sort = getActiveCustomSmartAlbum()?.sort;
  return !!sort && (
    Number(sort.type) !== Number(config.search.sortType || 0) ||
    Number(sort.order) !== Number(config.search.sortOrder || 0)
  );
});

const isSmartAlbumGroupOverride = computed(() => {
  if (!isSmartAlbumView.value || isRandomSort.value) return false;
  const group = getActiveCustomSmartAlbum()?.group;
  return !!group && Number(group.type) !== Number(config.search.groupBy ?? GROUP.NONE);
});

// update image when the select file is changed
async function updateSelectedImage(index: number) {
  if(index < 0 || index >= fileList.value.length) return;

  const file = fileList.value[index];
  if (!isRealFileItem(file) || !isInfoPanelOpen.value || !file.has_tags) return;

  // Keep a stable reference because virtual-list slots can be replaced while
  // the tag query is in flight.
  const tags = await getTagsForFile(file.id);
  if (fileList.value[index] === file) {
    file.tags = tags;
  }
}

// click ok in tagging dialog
async function updateFileHasTags(fileStates: Array<{ file_id: number; has_tags: boolean }>) {
  await syncTagStates(fileStates);
  showTaggingDialog.value = false;
}

async function syncTagStates(fileStates: Array<{ file_id: number; has_tags: boolean }>) {
  const filesById = new Map(
    fileList.value
      .filter(file => isRealFileItem(file))
      .map(file => [Number(file.id), file]),
  );
  for (const state of fileStates) {
    const file = filesById.get(Number(state.file_id));
    if (!file) continue;
    file.has_tags = Boolean(state.has_tags);
    file.tags = undefined;
  }

  const activeFile = fileList.value[selectedItemIndex.value];
  const activeState = fileStates.find(state => Number(state.file_id) === Number(activeFile?.id));
  if (activeFile && activeState) {
    activeFile.tags = activeState.has_tags ? ((await getTagsForFile(activeFile.id)) || []) : [];
    syncFileMetaToImageViewer(activeFile.id, {
      has_tags: activeFile.has_tags,
      tags: activeFile.tags,
    });
  }
}

async function handleCollectionsAdded({ fileIds = [], results, removedCollectionIds = [], failed = 0 }: { fileIds?: number[]; results: any[]; removedCollectionIds?: number[]; failed?: number }) {
  showAddToCollectionDialog.value = false;
  const addedFileIds = new Set(results.flatMap(result => result?.addedFileIds || []).map(Number));
  const skippedFileIds = new Set(results.flatMap(result => result?.skippedFileIds || []).map(Number));
  addedFileIds.forEach(fileId => skippedFileIds.delete(fileId));
  if (addedFileIds.size > 0) toast.success(t('collection.added_toast', { count: addedFileIds.size }));
  if (skippedFileIds.size > 0) toast.info(t('collection.already_exists_toast', { count: skippedFileIds.size }));
  if (addedFileIds.size > 0) {
    fileList.value.forEach(file => {
      if (!addedFileIds.has(Number(file?.id))) return;
      file.has_collections = true;
      file.collectionVersion = Number(file.collectionVersion || 0) + 1;
    });
    void tauriEmit('collection-files-dropped', { fileIds: [...addedFileIds] });
  }
  if (removedCollectionIds.length > 0) {
    const removedFileIds = fileIds.map(Number).filter((id: number) => id > 0 && !addedFileIds.has(id));
    for (const fileId of removedFileIds) {
      const collections = await getFileCollections(fileId);
      const hasCollections = Array.isArray(collections) && collections.length > 0;
      const file = fileList.value.find(f => Number(f?.id) === fileId);
      if (!file) continue;
      file.has_collections = hasCollections;
      // Bump even when still in another collection so FileInfo re-fetches the
      // now-shrunk membership list, not just when the badge flips on/off.
      file.collectionVersion = Number(file.collectionVersion || 0) + 1;
    }
  }
  if (currentQuerySource.value === 'collection' && removedCollectionIds.includes(Number(currentCollectionId.value))) {
    const removedIds = new Set(fileIds.map(Number));
    if (groupedModeActive.value) {
      void updateContent(true);
    } else {
      fileList.value = fileList.value.filter(file => !removedIds.has(Number(file.id)));
      totalFileCount.value = fileList.value.length;
      totalFileSize.value = fileList.value.reduce((total, file) => total + Number(file.size || 0), 0);
      selectedItemIndex.value = fileList.value.length > 0 ? Math.min(selectedItemIndex.value, fileList.value.length - 1) : -1;
    }
    void tauriEmit('collection-files-dropped', { collectionId: currentCollectionId.value });
  }
  if (failed > 0) toast.error(t('collection.add_failed_toast', { count: failed }));
}

function handleCollectionDeleted(collectionId: number) {
  if (Number(currentCollectionId.value) === Number(collectionId)) {
    libConfig.activePane = 'main';
    libConfig.collection.selectedId = null;
    config.main.sidebarIndex = SIDEBAR.LIBRARY;
    libConfig.library.item = LIB_ITEM.ALL;
  }
  // Deleting a collection changes has_collections for every file in it, so
  // refresh the file list to clear stale collection badges / membership.
  void tauriEmit('refresh-content');
}

// Track current thumbnail request to enable cancellation when switching folders
let currentThumbRequestId = 0;
let thumbnailRefreshVersion = 0;
let thumbnailPriorities = new Map<number, number>();
const thumbnailScheduler = createThumbnailScheduler({
  // Cache-read IPC has a separate budget from backend background generation.
  priority: (task: any) => task.requestId !== currentThumbRequestId ? null
    : thumbnailPriorities.get(Number(task.file.id)) ?? (task.viewportBound ? null : 2),
  loadBatch: (tasks: any[]) => getFileThumbs(tasks.map(({ file }) => ({
    fileId: file.id, filePath: file.file_path, fileType: file.file_type,
    orientation: file.e_orientation || 0, albumId: file.album_id || 0,
  })), tasks[0].thumbnailSize, false, tasks[0].trustCached),
});

function invalidateThumbnailRequests() {
  currentThumbRequestId++;
  thumbnailPriorities.clear();
  thumbnailScheduler.refresh();
}

function refreshThumbnailViewport() {
  const start = Math.max(0, lastVisibleRange.start);
  const end = lastVisibleRange.end >= start ? lastVisibleRange.end
    : start + Math.max(1, visibleItemCount.value) - 1;
  const buffer = Math.max(40, Math.min(visibleItemCount.value, 120));
  const rows = gridRows.value;
  const priorities = new Map<number, number>();
  for (let index = Math.max(0, start - buffer); index <= Math.min(rows.length - 1, end + buffer); index++) {
    const row = rows[index];
    const file = isItemRow(row) ? row.file : row;
    if (isRealFileItem(file)) priorities.set(Number(file.id), index >= start && index <= end ? 0 : 1);
  }
  thumbnailPriorities = priorities;
  thumbnailScheduler.refresh();
}

onBeforeUnmount(() => {
  currentThumbRequestId++;
  thumbnailScheduler.dispose();
});
// A reconnected album can safely show its existing local thumbnail cache first.
// A later scan/sync remains responsible for detecting files changed while offline.
let trustCachedThumbnailRequestId = -1;

function preserveLoadedThumbnails(files: any[]) {
  const thumbnailsById = new Map<number, string>();
  for (const file of fileList.value || []) {
    const fileId = Number(file?.id || 0);
    if (fileId > 0 && file?.thumbnail && !file.rawThumbnailStale) {
      thumbnailsById.set(fileId, file.thumbnail);
    }
  }

  return (files || []).map((file: any) => {
    const fileId = Number(file?.id || 0);
    if (!file?.thumbnail && fileId > 0) {
      const thumbnail = thumbnailsById.get(fileId);
      if (thumbnail) return { ...file, thumbnail };
    }
    return file;
  });
}

async function handleThumbnailReady(event: any) {
  const { file_ids, invalidate = true, library_id, thumbnail_only = false } = event.payload || {};
  const libraryId = libConfig._libraryId;
  const requestId = currentThumbRequestId;
  const isCurrent = () => libraryId === libConfig._libraryId && requestId === currentThumbRequestId;
  if (library_id && library_id !== libraryId) return;
  if (!Array.isArray(file_ids) || file_ids.length === 0) return;

  const readyIds = new Set(
    file_ids.map((id: any) => Number(id)).filter((id: number) => Number.isFinite(id) && id > 0)
  );
  if (readyIds.size === 0) return;
  // A companion edit changes the grouped RAW display even though its own
  // modified_at timestamp and logical selection ID remain unchanged.
  for (const file of fileList.value) {
    if (file && !file.isPlaceholder && file.media_subtype === 'raw_jpeg_pair' && readyIds.has(Number(file.live_photo_video_id))) {
      readyIds.add(Number(file.id));
    }
  }

  // A normal scan emits this event when the thumbnail first becomes ready.
  // The streaming list is already fetching that image, so clearing it here
  // would turn a successful first paint into a second, visible load. Only
  // invalidation events represent an existing thumbnail being replaced.
  if (!invalidate) {
    if (fileList.value.length === 0) return;
    const missingFiles = fileList.value.filter(
      (file: any) => file && !file.isPlaceholder && (!file.thumbnail || file.rawThumbnailStale) && readyIds.has(Number(file.id || 0))
    );
    if (missingFiles.length > 0) await getFileListThumb(missingFiles, { viewportBound: false });
    return;
  }

  // The backend has just replaced these thumbnails after detecting a changed
  // file. Drop their data-URL entries even when this view is empty, so a
  // later list render cannot reuse an outdated image from memory.
  for (const fileId of readyIds) {
    clearCachedThumbnailDataUrl(fileId, config.settings.thumbnailSize);
  }
  if (fileList.value.length === 0) return;

  const loadedFiles = fileList.value.filter(
    (file: any) => file && !file.isPlaceholder && readyIds.has(Number(file.id || 0))
  );
  if (loadedFiles.length === 0) return;

  // Pick up the new modified_at value before rebuilding URLs; it is the
  // persistent cache version for this file's contents.
  await Promise.all(loadedFiles.map(async (file: any) => {
    const refreshed = await getFileInfo(Number(file.id));
    if (isCurrent() && refreshed && fileList.value.includes(file)) Object.assign(file, refreshed);
  }));

  if (!isCurrent()) return;

  // Reload the regenerated thumbnails instead of retaining the old values
  // that arrived with the refreshed file list.
  const currentFiles = loadedFiles.filter((file: any) => fileList.value.includes(file));
  for (const file of currentFiles) {
    file.thumbnail = '';
  }

  // Match Refresh file info: changing filePath makes Image.vue reload the
  // currently displayed file after its on-disk contents have changed.
  const activeFile = fileList.value[selectedItemIndex.value];
  if (!thumbnail_only && activeFile && readyIds.has(Number(activeFile.id || 0)) && activeFile.file_path) {
    const activePath = activeFile.file_path;
    activeFile.file_path = '';
    await nextTick();
    if (activeFile.file_path === '') activeFile.file_path = activePath;
  }

  if (isCurrent()) await getFileListThumb(currentFiles, { bustCache: true, viewportBound: false });
}

// All grid loads share one queue with centrally capped batch size and concurrency.
async function getFileListThumb(files: any[], { bustCache = false, viewportBound = true } = {}) {
  const requestId = currentThumbRequestId;
  const thumbnailSize = config.settings.thumbnailSize;
  const trustCached = requestId === trustCachedThumbnailRequestId;
  const refreshVersion = bustCache ? ++thumbnailRefreshVersion : 0;
  const group = `${requestId}:${thumbnailSize}:${trustCached}:${refreshVersion}`;
  refreshThumbnailViewport();

  const candidates = files.filter((file: any) => isRealFileItem(file)
    && (!file.thumbnail || file.rawThumbnailStale)
    && (!viewportBound || thumbnailPriorities.has(Number(file.id))));
  const load = async (file: any) => {
    const cached = getCachedThumbnailDataUrl(file.id, thumbnailSize);
    if (cached) {
      file.thumbnail = cached;
      file.rawThumbnailStale = false;
      return;
    }
    try {
      // group includes the refresh version, keeping forced refreshes independent.
      const key = `${group}:${file.id}:${file.modified_at || 0}`;
      const thumb = await thumbnailScheduler.request(key, group, {
        file, requestId, thumbnailSize, trustCached, viewportBound,
      });
      if (!thumb || requestId !== currentThumbRequestId) return;
      if (thumb.error_code === 0 || thumb.error_code === 2) {
        file.thumbnail = getThumbnailDataUrl(thumb, thumbnailPlaceholder, bustCache, thumbnailSize, file.file_path, Number(file.modified_at || 0), !file.cloud_only);
        file.rawThumbnailStale = false;
      } else if (thumb.error_code === 1) {
        file.rawThumbnailStale = false;
        file.thumbnail = thumbnailPlaceholder;
      }
    } catch (error) {
      console.log('Error fetching thumbnail:', error);
    }
  };
  // Large explicit refreshes retain offscreen demand, but enqueue just one batch
  // at a time. Visible work can take priority before the next batch is submitted.
  const windowSize = viewportBound ? Math.max(1, candidates.length) : 4;
  for (let start = 0; start < candidates.length; start += windowSize) {
    if (requestId !== currentThumbRequestId) return;
    await Promise.all(candidates.slice(start, start + windowSize).map(load));
  }
}

// Home uses an independent bounded source with the normal single-photo viewer.
async function openHomePhoto(request: { id: number; ids: number[]; libraryId: string }) {
  if (request.libraryId !== libConfig._libraryId || config.main.sidebarIndex !== SIDEBAR.HOME) return;
  const ids = homePhotoIds(request.id, request.ids);
  if (!ids.length) return;
  const files = await invoke<any[]>('get_home_preview_files', { libraryId: request.libraryId, fileIds: ids });
  if (request.libraryId !== libConfig._libraryId || config.main.sidebarIndex !== SIDEBAR.HOME) return;
  const byId = new Map((files || []).map((file: any) => [Number(file.id), file]));
  const snapshot = ids.map(id => byId.get(id)).filter(Boolean);
  const index = snapshot.findIndex((file: any) => file.id === request.id);
  if (index < 0) throw new Error(t('home_dashboard.photo_unavailable'));
  await openImageViewer(index, true, false, { files: snapshot });
}

let chatMediaNavigation = 0;
async function handleChatMediaNavigation(request: { libraryId: string; fileId: number; start?: number | null }) {
  const libraryId = libConfig._libraryId;
  if (request.libraryId !== libraryId || !Number.isSafeInteger(request.fileId) || request.fileId <= 0) return;
  const navigation = ++chatMediaNavigation;
  const isCurrent = () => navigation === chatMediaNavigation && libraryId === libConfig._libraryId;
  const files = await invoke<any[]>('get_home_preview_files', { libraryId, fileIds: [request.fileId] });
  if (!isCurrent()) return;
  const file = files.find(item => Number(item.id) === request.fileId);
  if (!file) throw new Error(t('media_chat.media_unavailable'));
  const seek = Number(file.file_type) === 2 && typeof request.start === 'number' && Number.isFinite(request.start) && request.start >= 0 ? request.start : undefined;
  await openImageViewer(0, true, false, { files: [file], showInfo: true, libraryId, seek, isCurrent });
}

// Open the image viewer window
async function openImageViewer(
  index: number,
  newViewer = false,
  syncFromFileListChange = false,
  options: { rightIndex?: number; forceSplitCount?: 2 | 4; files?: any[]; showInfo?: boolean; libraryId?: string; seek?: number; isCurrent?: () => boolean } = {}
) {

  if (options.libraryId && options.libraryId !== libConfig._libraryId || options.isCurrent?.() === false) return;

  const webViewLabel = 'imageviewer';

  const isCompareRequest = Array.isArray(options.files);
  // Content refreshes are for the normal viewer only. Preserve an active
  // comparison session until the user explicitly opens a normal viewer again.
  if (!isCompareRequest && imageViewerSession.value.mode === 'compare' && !newViewer) return;

  if (isCompareRequest) {
    imageViewerSession.value = {
      mode: 'compare',
      files: options.files!,
    };
  } else {
    imageViewerSession.value = { mode: 'normal' };
  }

  const viewerObjectClassKey = objectBoxClassKey.value;
  const viewerFiles = isCompareRequest ? options.files! : fileList.value;
  const fileCount = viewerFiles.length;
  const isRealFile = (item: any) => !!item && !item.isPlaceholder && typeof item.id === 'number';
  const getRealFileAt = (targetIndex: number) => {
    if (targetIndex < 0 || targetIndex >= fileCount) return null;
    const file = viewerFiles[targetIndex];
    return isRealFile(file) ? file : null;
  };
  const getNextImageFilePath = (targetIndex: number) => {
    const file = getRealFileAt(targetIndex + 1);
    return file?.file_type === 1 ? file.file_path : '';
  };

  let leftIndex = index;
  let rightIndex = -1;

  if (syncFromFileListChange) {
    if (fileCount === 0) {
      leftIndex = -1;
      rightIndex = -1;
    } else if (fileCount === 1) {
      leftIndex = 0;
      rightIndex = 0;
    } else {
      leftIndex = 0;
      rightIndex = 1;
    }
  }
  if (typeof options.rightIndex === 'number') {
    rightIndex = options.rightIndex;
  }
  const compareSplitCount = options.forceSplitCount;
  const compareMode = typeof compareSplitCount === 'number';
  if (compareSplitCount) {
    if (rightIndex < 0 && fileCount > 0) {
      rightIndex = Math.min(leftIndex + 1, fileCount - 1);
    }
  }

  const leftFile = getRealFileAt(leftIndex);
  const rightFile = getRealFileAt(rightIndex);
  const leftFileId = leftFile ? leftFile.id : 0;
  const rightFileId = rightFile ? rightFile.id : 0;
  const leftNextFilePath = getNextImageFilePath(leftIndex);
  const rightNextFilePath = getNextImageFilePath(rightIndex);
  
  // create a new window if it doesn't exist
  let imageWindow = await WebviewWindow.getByLabel(webViewLabel);
  if (options.libraryId && options.libraryId !== libConfig._libraryId || options.isCurrent?.() === false) return;
  if (!imageWindow) {
    if (newViewer) {
      const forceSplitParam = compareSplitCount || 0;
      const compareModeParam = compareMode ? 1 : 0;
      const proxyOptions = await startupWindowProxy();
      if (options.libraryId && options.libraryId !== libConfig._libraryId || options.isCurrent?.() === false) return;
      imageWindow = new WebviewWindow(webViewLabel, {
        ...proxyOptions,
        url: `/image-viewer?fileId=${leftFileId}&fileIndex=${leftIndex}&fileCount=${fileCount}&rightFileId=${rightFileId}&rightFileIndex=${rightIndex}&forceSplitCount=${forceSplitParam}&compareFileCount=${compareMode ? fileCount : 0}&compareMode=${compareModeParam}&nextFilePath=${encodeURIComponent(leftNextFilePath)}&rightNextFilePath=${encodeURIComponent(rightNextFilePath)}&objectClassKey=${encodeURIComponent(viewerObjectClassKey)}&showInfo=${options.showInfo ? 1 : 0}&sourceLibraryId=${encodeURIComponent(options.libraryId || '')}&seek=${options.seek ?? ''}`,
        title: 'Image Viewer',
        width: 1200,
        height: 800,
        minWidth: 800,
        minHeight: 600,
        resizable: true,
        visible: false, // Start hidden, will show after mount
        transparent: true, // Prevent white flash on show (Tauri 2.x workaround)
        decorations: isMac,
        zoomHotkeysEnabled: true, // Windows WebView2: needed for touchpad pinch-to-zoom
        ...(isMac && {
          titleBarStyle: 'overlay',
          hiddenTitle: true,
          minimizable: true,
        }),
      });

      imageWindow.once('tauri://created', () => {
        isImageViewerWindowOpen.value = true;
        console.log('ImageViewer window created');
        videoRef.value?.pause();  // pause video playing in preview pane
      });

      imageWindow.once('tauri://destroyed', () => {
        isImageViewerWindowOpen.value = false;
      });

      imageWindow.once('tauri://error', (e) => {
        console.error('Error creating ImageViewer window:', e);
      });
    }
  } else {    // update the existing window
    isImageViewerWindowOpen.value = true;
    await imageWindow.emit('update-img', { 
      fileId: leftFileId, 
      libraryId: options.libraryId,
      showInfo: options.showInfo,
      seek: options.seek,
      objectClassKey: viewerObjectClassKey,
      fileIndex: leftIndex,   // selected file index
      fileCount: fileCount, // total files length
      nextFilePath: leftNextFilePath,
      pane: 'left',
      resetSplit: newViewer,
      compareMode: newViewer ? compareMode : undefined,
      compareFileCount: compareMode ? fileCount : undefined,
      forceSplitCount: compareSplitCount,
      // filePath: encodedFilePath, 
      // nextFilePath: nextEncodedFilePath,
    });

    if (syncFromFileListChange || rightIndex >= 0) {
      await imageWindow.emit('update-img', {
        fileId: rightFileId,
        fileIndex: rightIndex,
        fileCount: fileCount,
        nextFilePath: rightNextFilePath,
        pane: 'right',
      });
    }

    if (compareMode && compareSplitCount === 4) {
      const emitComparisonPane = async (pane: 'bottomLeft' | 'bottomRight', paneIndex: number) => {
        const paneFile = getRealFileAt(paneIndex);
        await imageWindow.emit('update-img', {
          fileId: paneFile?.id || 0,
          fileIndex: paneFile ? paneIndex : -1,
          fileCount,
          nextFilePath: paneFile ? getNextImageFilePath(paneIndex) : '',
          pane,
        });
      };
      await emitComparisonPane('bottomLeft', 2);
      // A three-image comparison intentionally leaves the fourth pane blank.
      await emitComparisonPane('bottomRight', 3);
    }

    if(newViewer) {
      imageWindow.show();
      if (options.showInfo) await imageWindow.setFocus();
    }
    videoRef.value?.pause();  // pause video playing in preview pane
  }
}

// Push the main-window selection to an open (normal-mode) image viewer, reusing
// the existing update-img channel. Gated on isImageViewerWindowOpen so a
// selection change does no async window lookup when no viewer is open, and it
// re-checks the selection after the await so the latest index wins when the
// user navigates rapidly (out-of-order getByLabel resolutions cannot regress
// the viewer to a stale image).
async function syncSelectionToImageViewer(index: number) {
  if (!isImageViewerWindowOpen.value) return;
  if (imageViewerSession.value.mode !== 'normal') return;
  if (!isRealFileItem(fileList.value[index])) return;
  const imageWindow = await WebviewWindow.getByLabel('imageviewer');
  if (!imageWindow || selectedItemIndex.value !== index) return;
  const file = fileList.value[index];
  const next = fileList.value[index + 1];
  imageWindow.emit('update-img', {
    fileId: file.id,
    objectClassKey: objectBoxClassKey.value,
    fileIndex: index,
    fileCount: fileList.value.length,
    nextFilePath: next && !next.isPlaceholder && next.file_type === 1 ? next.file_path : '',
    pane: 'left',
  });
}

let montageSaveAsContext: SaveAsContext | null = null;
let montageSourceFolder = '';

// selected images, including the ones not loaded in the view yet
async function getMontageImages() {
  return ((await getActionableSelectedItemsForAction()) || []).filter((item: any) => item.file_type !== 2);
}

let montageOpening = false;

async function openMontage() {
  if (!await requireCurrentOriginals()) return;
  // one montage at a time: bring the open one to the front
  if (montageOpening) return;
  const openWindow = await WebviewWindow.getByLabel('montage');
  if (openWindow) {
    await openWindow.unminimize();
    await openWindow.setFocus();
    return;
  }
  montageOpening = true;
  try {
    await createMontageWindow();
  } finally {
    montageOpening = false;
  }
}

async function createMontageWindow() {
  const images = await getMontageImages();
  const imageIds = images.map((item: any) => Number(item.id));
  if (imageIds.length < 2) {
    toast.warning(t('msgbox.montage.not_enough_photos'));
    return;
  }
  if (imageIds.length > MONTAGE_MAX_PHOTOS) {
    toast.warning(t('msgbox.montage.too_many_photos', { count: MONTAGE_MAX_PHOTOS }));
  }

  // the save dialog proposes the first photo's folder, like the image editor's "save as new"
  montageSaveAsContext = getCurrentSaveAsContext(images[0]);
  montageSourceFolder = getFolderPath(images[0].file_path);

  const parentWindow = getCurrentWebviewWindow();
  const [size, position, scaleFactor] = await Promise.all([
    parentWindow.innerSize(), parentWindow.outerPosition(), parentWindow.scaleFactor(),
  ]);
  const { width, height } = size.toLogical(scaleFactor);
  const { x, y } = position.toLogical(scaleFactor);

  const newWindow = new WebviewWindow('montage', {
    ...await startupWindowProxy(),
    url: `/montage?fileIds=${imageIds.slice(0, MONTAGE_MAX_PHOTOS).join(',')}`,
    title: 'Montage',
    width,
    height,
    x,
    y,
    minWidth: 800,
    minHeight: 500,
    resizable: true,
    maximizable: false,
    visible: false,
    transparent: true,
    decorations: isMac,
    ...(isMac && {
      titleBarStyle: 'overlay',
      hiddenTitle: true,
      minimizable: false,
    }),
  });

  // wait until the window exists, so a second click finds it
  await new Promise<void>(resolve => {
    newWindow.once('tauri://created', () => {
      newWindow.show();
      resolve();
    });
    newWindow.once('tauri://error', () => resolve());
  });
}

async function openImageEditor(index: number) {
  const file = fileList.value[index];
  if (!file || !await requireOriginalFiles([file])) return;
  const fileId = Number(file.id || 0);
  if (fileId <= 0) return;

  imageEditorSaveAsContexts.set(fileId, getCurrentSaveAsContext(file));

  const webViewLabel = 'imageeditor';
  const imageWindow = await WebviewWindow.getByLabel(webViewLabel);
  if (imageWindow) {
    await imageWindow.emit('update-file', { fileId });
    imageWindow.show();
    imageWindow.setFocus();
    return;
  }

  const newWindow = new WebviewWindow(webViewLabel, {
        ...await startupWindowProxy(),
    url: `/image-editor?fileId=${fileId}`,
    title: 'Image Editor',
    width: 1100,
    height: 700,
    minWidth: 800,
    minHeight: 500,
    resizable: true,
    maximizable: false,
    visible: false,
    transparent: true,
    decorations: isMac,
    ...(isMac && {
      titleBarStyle: 'overlay',
      hiddenTitle: true,
      minimizable: false,
    }),
  });

  newWindow.once('tauri://created', () => {
    newWindow?.show();
  });

}

const printImageView = ref<InstanceType<typeof PrintImage> | null>(null);

async function printImage(index: number) {
  const selectedFile = fileList.value[index];
  if (!selectedFile?.file_path || !await requireOriginalFiles([selectedFile])) return;

  const fileId = Number(selectedFile.id || 0);
  const fileType = Number(selectedFile.file_type || 1);

  try {
    const source = shouldUseBackendPreview(selectedFile.file_path, fileType)
      ? getPreviewUrl(
        fileId,
        selectedFile.file_path,
        false,
        Number(selectedFile.modified_at || 0),
      )
      : getAssetSrc(selectedFile.file_path, Number(selectedFile.modified_at || 0));
    await printImageView.value?.print(source);
  } catch (error) {
    console.error('Failed to prepare image for printing:', error);
  }
}

/// Dragging the film strip view splitter
// function startDraggingfilmStripView(event: MouseEvent) {
//   isDraggingFilmStripView.value = true;
//   document.addEventListener('mousemove', handleMouseMove);
//   document.addEventListener('mouseup', stopDragging);
// }

/// Dragging the info panel splitter
function startDraggingInfoPanelSplitter(event: MouseEvent) {
  isDraggingInfoPanel.value = true;
  rightPanelDragStartX.value = event.clientX;
  rightPanelDragStartWidth.value = Number(config.rightPanel.width || 360);
  document.addEventListener('mousemove', handleMouseMove);
  document.addEventListener('mouseup', stopDragging);
}

/// handle mouse move event
function handleMouseMove(event: MouseEvent) {
  if (isDraggingInfoPanel.value) {
    const deltaX = rightPanelDragStartX.value - event.clientX;
    const newWidth = rightPanelDragStartWidth.value + deltaX;
    config.rightPanel.width = clampRightPanelWidth(newWidth);
  }
}

function stopDragging() {
  // isDraggingFilmStripView.value = false;
  isDraggingInfoPanel.value = false;
  rightPanelDragStartX.value = 0;
  rightPanelDragStartWidth.value = 0;
  document.removeEventListener('mousemove', handleMouseMove);
  document.removeEventListener('mouseup', stopDragging);
}

const emit = defineEmits<{ startupReady: [] }>();
let startupReadyEmitted = false;
watch([contentReady, showWelcomeContent], ([ready, welcome]) => {
  if (startupReadyEmitted || (!ready && !welcome)) return;
  startupReadyEmitted = true;
  void nextTick(() => requestAnimationFrame(() => requestAnimationFrame(() => emit('startupReady'))));
}, { immediate: true });

defineExpose({
  selectedMediaChatIds,
  openHomePhoto,
  prepareSettingsOpen,
  focusContent: activateContentPane,
  refreshCenteredGridLayout: refreshGridLayoutPreservingScroll,
});
useMediaViewEvents(change => {
  if (change.library_id !== libConfig._libraryId) return;
  for (const file of fileList.value) applyMediaViewChange(file, libConfig._libraryId, change);
  for (const row of groupedRows.value) applyMediaViewChange(row?.file || row?.item || row, libConfig._libraryId, change);
  if (imageViewerSession.value.mode === 'compare') {
    for (const file of imageViewerSession.value.files) applyMediaViewChange(file, libConfig._libraryId, change);
  }
});
useMediaTitleEvents(change => {
  if (change.library_id !== libConfig._libraryId) return;
  for (const file of fileList.value) applyMediaTitleChange(file, libConfig._libraryId, change);
  for (const row of groupedRows.value) applyMediaTitleChange(row?.file || row?.item || row, libConfig._libraryId, change);
  if (imageViewerSession.value.mode === 'compare') {
    for (const file of imageViewerSession.value.files) applyMediaTitleChange(file, libConfig._libraryId, change);
  }
});
const mediaAiStops: Array<() => void> = [];
let mediaAiDisposed = false;
onMounted(async () => {
  const stopMeta = await listen<any>('media-ai-metadata-changed', event => {
    if (event.payload.libraryId !== libConfig._libraryId) return;
    for (const file of fileList.value) if (Number(file.id) === event.payload.fileId) { file.title = event.payload.title; file.comments = event.payload.comments; }
  });
  if (mediaAiDisposed) stopMeta(); else mediaAiStops.push(stopMeta);
  const stopHit = await listen<any>('media-ai-open-hit', async event => {
    if (event.payload.libraryId !== libConfig._libraryId) return;
    checkUnsavedChanges(async () => {
      if (event.payload.libraryId !== libConfig._libraryId) return;
      const index = fileList.value.findIndex(file => Number(file.id) === event.payload.fileId);
      if (index < 0) return;
      selectedItemIndex.value = index; await seekMediaAiVideo(event.payload.seconds);
    });
  });
  if (mediaAiDisposed) stopHit(); else mediaAiStops.push(stopHit);
});
onBeforeUnmount(() => { if (mediaAiSearchRequest) void invoke('media_ai_cancel_request', { requestId: mediaAiSearchRequest }); mediaAiDisposed = true; mediaAiStops.forEach(stop => stop()); });
</script>

<style scoped>
.selection-bar-enter-active,
.selection-bar-leave-active {
  transition: opacity 0.2s ease-out, transform 0.2s ease-out;
}

.selection-bar-enter-from,
.selection-bar-leave-to {
  opacity: 0;
  transform: translateY(4px);
}

.selection-bar-leave-active {
  pointer-events: none;
}

@media (prefers-reduced-motion: reduce) {
  .selection-bar-enter-active,
  .selection-bar-leave-active { transition: none; }
}

.drop-overlay {
  position: absolute;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(59, 130, 246, 0.12);
  border: 3px dashed rgba(59, 130, 246, 0.5);
  border-radius: 8px;
  pointer-events: none;
  backdrop-filter: blur(2px);
}

.drop-overlay-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  color: rgba(59, 130, 246, 0.9);
  font-size: 1.1rem;
  font-weight: 500;
}
</style>

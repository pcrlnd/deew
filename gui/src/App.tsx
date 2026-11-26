import {
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react"
import { open } from "@tauri-apps/plugin-dialog"
import { getCurrentWebview } from "@tauri-apps/api/webview"

import { CommandBar } from "@/components/command-bar"
import { ConfirmConfigDialog } from "@/components/confirm-config-dialog"
import { ConfirmUpdateDialog } from "@/components/confirm-update-dialog"
import { NoteLine } from "@/components/note-line"
import { ProgressPane } from "@/components/progress-pane"
import { PromptDialog } from "@/components/prompt-dialog"
import { runOutcome, type RunOutcome } from "@/lib/run-outcome"
import {
  FileBrowserDialog,
  type BrowseRequest,
} from "@/components/file-browser-dialog"
import { OptionsForm } from "@/components/options-form"
import { TerminalPane, type TerminalHandle } from "@/components/terminal-pane"
import { TitleBar } from "@/components/title-bar"
import { UtilitiesPane } from "@/components/utilities-pane"
import { TooltipProvider } from "@/components/ui/tooltip"
import { WebToolbar } from "@/components/web-toolbar"
import {
  DEFAULT_BITRATE,
  DEFAULT_OPTIONS,
  PROGRAM_DEFAULT_MODES,
  PROGRAM_DD_FILTER_DEFAULTS,
  ddFilterDefaultsOf,
  type DdFilterDefaults,
  buildArgs,
  defaultBitrate,
  detectPlatform,
  effectiveDdpAtmosMode,
  effectiveDdpMode,
  formatCommand,
  infoArgs,
  parseDefaultModes,
  setAc4ImsMode,
  normalizeTrackIndex,
  relevantBitrates,
  relevantDownmixes,
  validate,
  type DeewOptions,
  type DefaultModes,
  type InfoFlag,
} from "@/lib/deew-args"
import {
  errorStatus,
  exitStatus,
  finishedStatus,
  prompt,
  stoppedStatus,
  successStatus,
} from "@/lib/terminal-format"
import {
  EMPTY_HISTORY,
  parseProgressEvent,
  reduceHistory,
} from "@/lib/progress"
import {
  DEFAULT_TEMPLATE,
  outputTemplateOf,
  previewNames,
} from "@/lib/output-name"
import { stateAction } from "@/lib/run-client"
import { useProbe } from "@/lib/use-probe"
import {
  activeTab,
  encodeBlock,
  isUpdate,
  needsConfirmation,
  utilityBlock,
  type OutputTab,
} from "@/lib/utilities"
import { errorMessage } from "@/lib/web-api"
import {
  cliFound,
  cliVersion,
  connectRunEvents,
  defaultBitrates,
  guiSettings,
  inTauri,
  inWeb,
  reconnectRunSocket,
  resizeInfo,
  writeInfo,
  resizePty,
  runDeew,
  runInfo,
  saveGuiSettings,
  saveGuiTheme,
  stopDeew,
  stopInfo,
  writeStdin,
  type OutputView,
} from "@/lib/runner"
import { parseThemeMode, setThemeMode, type ThemeMode } from "@/lib/theme"

const platform = detectPlatform(navigator.userAgent)

/** Resolves once the browser has laid out a change (or after 100 ms). */
function afterLayout(): Promise<void> {
  return new Promise((resolve) => {
    let done = false
    const finish = () => {
      if (!done) {
        done = true
        resolve()
      }
    }
    requestAnimationFrame(() => requestAnimationFrame(finish))
    setTimeout(finish, 100)
  })
}

function addUnique(list: string[], extra: string[]): string[] {
  const seen = new Set(list)
  const out = [...list]
  for (const p of extra) {
    if (!seen.has(p)) {
      seen.add(p)
      out.push(p)
    }
  }
  return out
}

export function App({ onLogout }: { onLogout?: () => void }) {
  const [options, setOptions] = useState<DeewOptions>({ ...DEFAULT_OPTIONS })
  const [running, setRunning] = useState(false)
  const [stopping, setStopping] = useState(false)
  const [outcome, setOutcome] = useState<RunOutcome | null>(null)
  const [endCount, setEndCount] = useState(0)
  const [dragging, setDragging] = useState(false)
  const terminal = useRef<TerminalHandle>(null)
  // The Utilities tab: its own terminal and its own (one) running command,
  // independent of the encode above.
  const utilityTerminal = useRef<TerminalHandle>(null)
  const [utilitiesShown, setUtilitiesShown] = useState(false)
  const utilitiesShownRef = useRef(false)
  const [utility, setUtility] = useState<InfoFlag | null>(null)
  const utilityRef = useRef<InfoFlag | null>(null)
  const utilityStoppedRef = useRef(false)
  const [utilityUsed, setUtilityUsed] = useState(false)
  const [confirmConfig, setConfirmConfig] = useState(false)
  const [confirmUpdate, setConfirmUpdate] = useState<InfoFlag | null>(null)
  const runningRef = useRef(false)
  const stoppedRef = useRef(false)
  // When this tab started the job; null for a job it only attached to.
  const startedAtRef = useRef<number | null>(null)
  // Web: this tab sent `start` and the server has not confirmed it yet.
  const awaitingStartRef = useRef(false)
  // Web: server events are handled strictly in order (attaching is async).
  const eventChain = useRef<Promise<void>>(Promise.resolve())
  const [browseRequest, setBrowseRequest] = useState<BrowseRequest | null>(null)
  const versionRef = useRef<string | null>(null)
  // Whether an empty prompt is the last thing in the terminal.
  const promptShownRef = useRef(false)
  // Output view: loaded from the config at startup, saved on every toggle.
  const [view, setView] = useState<OutputView>("cli")
  const viewRef = useRef<OutputView>("cli")
  const [saveError, setSaveError] = useState<string | null>(null)
  const [translocated, setTranslocated] = useState(false)
  const [outputMinHeight, setOutputMinHeight] = useState(320)
  const [nativeTitleBar, setNativeTitleBar] = useState(false)
  const [cliMissing, setCliMissing] = useState(false)
  const cliMissingRef = useRef(false)
  // The output directory as last loaded from / sent to the config.
  const savedOutputRef = useRef("")
  const outputTouchedRef = useRef(false)
  const outputRef = useRef("")
  // The same for "Save next to input" (output.to_input), saved at once.
  const savedToInputRef = useRef(false)
  const toInputTouchedRef = useRef(false)
  const toInputRef = useRef(false)
  const [ddFilterDefaults, setDdFilterDefaults] = useState<DdFilterDefaults>({
    ...PROGRAM_DD_FILTER_DEFAULTS,
  })
  const [defaultModes, setDefaultModes] = useState<DefaultModes>({
    ...PROGRAM_DEFAULT_MODES,
  })
  const [defaultOutput, setDefaultOutput] = useState("")
  // The config's `[output] template`: `-ot` is only passed when it differs.
  const [configTemplate, setConfigTemplate] = useState(DEFAULT_TEMPLATE)
  // Every run of this session, for the native view (like the terminal's).
  const [history, dispatchHistory] = useReducer(reduceHistory, EMPTY_HISTORY)
  const current = history.runs.at(-1)
  const [defaults, setDefaults] = useState<Record<string, number>>({})

  const native = view === "native"
  const showTerminal = !native
  const blockedReason = encodeBlock(utility)

  const trackIndex = normalizeTrackIndex(options.trackIndex)
  const probe = useProbe(options.inputs, trackIndex)
  // `-on` takes exactly one input file, counted after folders are expanded
  const singleFile =
    probe.status === "done"
      ? probe.summary.files + probe.summary.skipped.length === 1
      : options.inputs.length === 1
  const naming = useMemo(
    () => ({ configTemplate, singleFile }),
    [configTemplate, singleFile]
  )
  const channels = probe.status === "done" ? probe.summary.channels : null
  const codecs = probe.status === "done" ? probe.summary.codecs : []
  const inputs = probe.status === "done" ? probe.summary.inputs : undefined
  const downmixes = relevantDownmixes(options.format, channels)
  const downmix = downmixes.includes(options.downmix) ? options.downmix : "none"
  const bitrateInfo = relevantBitrates({
    format: options.format,
    channels,
    downmix,
    codecs,
    inputs,
    measureOnly: options.measureOnly,
    ddpMode: effectiveDdpMode(options.ddpMode, defaultModes),
    atmosMode: effectiveDdpAtmosMode(options.ddpAtmosMode, defaultModes),
  })
  const outputNames = useMemo(
    () =>
      options.measureOnly || probe.status !== "done"
        ? []
        : previewNames(
            probe.summary.entries,
            {
              options,
              configTemplate,
              atmosMode: effectiveDdpAtmosMode(
                options.ddpAtmosMode,
                defaultModes
              ),
              ddp71Bluray:
                bitrateInfo.ddp71 === true &&
                effectiveDdpMode(options.ddpMode, defaultModes) === "bluray",
              bitrate:
                options.bitrate === DEFAULT_BITRATE
                  ? defaultBitrate(bitrateInfo, defaults)
                  : Number(options.bitrate),
              track: trackIndex ?? "0",
            },
            singleFile
          ),
    [
      options,
      probe,
      configTemplate,
      defaultModes,
      bitrateInfo,
      defaults,
      trackIndex,
      singleFile,
    ]
  )
  const problems = useMemo(() => {
    const list = validate(options, defaultModes, naming)
    // what the CLI would refuse for the names it makes
    const refused = [
      ...new Set(
        outputNames.flatMap((p) =>
          "error" in p.result ? [`Output name: ${p.result.error}.`] : []
        )
      ),
    ]
    return [...list, ...refused.filter((m) => !list.includes(m))]
  }, [options, defaultModes, naming, outputNames])
  const canRun = options.inputs.length > 0 && problems.length === 0
  const args = buildArgs(options, {
    ddp71: bitrateInfo.ddp71,
    channels,
    defaultModes,
    ddFilterDefaults,
    naming,
  })
  const command = formatCommand(args, platform)
  // A downmix or bitrate that the probed layout cannot do goes back to the
  // default (adjusting state while rendering, as React recommends for derived
  // state).
  const bitrateInvalid =
    options.bitrate !== DEFAULT_BITRATE &&
    !bitrateInfo.bitrates.includes(Number(options.bitrate))
  if (downmix !== options.downmix || bitrateInvalid) {
    setOptions({
      ...options,
      downmix,
      bitrate: bitrateInvalid ? DEFAULT_BITRATE : options.bitrate,
    })
  }

  const addInputs = useCallback((paths: string[]) => {
    setOptions((o) => ({ ...o, inputs: addUnique(o.inputs, paths) }))
  }, [])

  // Drag & drop of files / folders onto the window.
  useEffect(() => {
    if (!inTauri()) return
    let unlisten: (() => void) | undefined
    let cancelled = false
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const p = event.payload
        if (p.type === "enter" || p.type === "over") setDragging(true)
        else if (p.type === "leave") setDragging(false)
        else if (p.type === "drop") {
          setDragging(false)
          if (!runningRef.current) addInputs(p.paths)
        }
      })
      .then((fn) => {
        if (cancelled) fn()
        else unlisten = fn
      })
      .catch((e) => console.error("drag & drop unavailable", e))
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [addInputs])

  const pick = async (directory: boolean, multiple: boolean) => {
    // No native dialogs in a browser: browse the server's file system.
    if (inWeb()) {
      return new Promise<string[]>((resolve) =>
        setBrowseRequest({
          directory,
          resolve: (paths) => {
            setBrowseRequest(null)
            resolve(paths)
          },
        })
      )
    }
    const selected = await open({ directory, multiple })
    if (selected === null) return []
    return Array.isArray(selected) ? selected : [selected]
  }

  // Without the CLI only "Update and install all" is offered; look again
  // whenever something may have installed it.
  const refreshCli = useCallback(() => {
    return cliFound().then((found) => {
      cliMissingRef.current = !found
      setCliMissing(!found)
    })
  }, [])
  useEffect(() => {
    let cancelled = false
    cliFound().then((found) => {
      if (cancelled) return
      cliMissingRef.current = !found
      setCliMissing(!found)
    })
    return () => {
      cancelled = true
    }
  }, [])

  const loadVersion = useCallback(async () => {
    if (versionRef.current) return
    try {
      versionRef.current = await cliVersion()
    } catch (error) {
      console.warn("deew version unavailable", error)
    }
  }, [])

  useEffect(() => {
    let cancelled = false
    guiSettings()
      .then((settings) => {
        if (cancelled) return
        setTranslocated(settings.translocated === true)
        setOutputMinHeight(settings.minimumOutputHeight)
        setNativeTitleBar(settings.titleBar === "native")
        if (settings.useSystemFont) {
          document.documentElement.dataset.font = "system"
        }
        // deew-web keeps its own mode per browser; the config wins in deew-gui.
        if (!inWeb()) {
          setThemeMode(parseThemeMode(settings.theme), { persist: true })
        }
        setDefaultOutput(settings.defaultOutput)
        const modes = parseDefaultModes(
          settings.defaultDdpMode,
          settings.defaultDdpAtmosMode,
          settings.defaultAc4ImsMode
        )
        setDefaultModes(modes)
        // a config with ac4_ims = 'music' needs the DRC music_light
        setOptions((o) =>
          o.format === "ac4-ims" ? setAc4ImsMode(o, o.ac4ImsMode, modes) : o
        )
        setDdFilterDefaults(ddFilterDefaultsOf(settings))
        setConfigTemplate(outputTemplateOf(settings))
        if (!toInputTouchedRef.current) {
          savedToInputRef.current = settings.toInput
          toInputRef.current = settings.toInput
          setOptions((o) => ({ ...o, outputToInput: settings.toInput }))
        }
        // An edit before the answer arrives wins.
        if (!outputTouchedRef.current) {
          savedOutputRef.current = settings.output
          outputRef.current = settings.output
          setOptions((o) => ({ ...o, output: settings.output }))
        }
        // A toggle before the answer arrives wins.
        if (viewRef.current !== "cli") return
        viewRef.current = settings.progress
        setView(settings.progress)
      })
      .catch((e) => console.warn("gui settings unavailable", e))
    return () => {
      cancelled = true
    }
  }, [])

  // Every save sends both settings.
  const saveSettings = useCallback(() => {
    saveGuiSettings({
      progress: viewRef.current,
      output: outputRef.current,
      toInput: toInputRef.current,
    }).then(
      () => setSaveError(null),
      (e) => setSaveError(errorMessage(e))
    )
  }, [])

  const saveTheme = useCallback((mode: ThemeMode) => {
    saveGuiTheme(mode).then(
      () => setSaveError(null),
      (e) => setSaveError(errorMessage(e))
    )
  }, [])

  const changeView = (next: OutputView) => {
    viewRef.current = next
    setView(next)
    saveSettings()
  }

  // Typing, Browse and Clear all end up here: save 500 ms after the last change.
  useEffect(() => {
    outputRef.current = options.output
    if (options.output === savedOutputRef.current) return
    outputTouchedRef.current = true
    const timer = setTimeout(() => {
      savedOutputRef.current = options.output
      saveSettings()
    }, 500)
    return () => clearTimeout(timer)
  }, [options.output, saveSettings])

  // The toggle is saved right away, the path stays as it is.
  useEffect(() => {
    toInputRef.current = options.outputToInput
    if (options.outputToInput === savedToInputRef.current) return
    toInputTouchedRef.current = true
    savedToInputRef.current = options.outputToInput
    saveSettings()
  }, [options.outputToInput, saveSettings])

  // Raw event lines of the CLI; anything unparsable is ignored.
  const onProgressLine = useCallback((line: string) => {
    const event = parseProgressEvent(line)
    if (event) dispatchHistory({ type: "event", event })
  }, [])

  // Adds the run that is starting / attached to the native view.
  const beginProgress = useCallback((runArgs: string[]) => {
    dispatchHistory({
      type: "begin",
      command: formatCommand(runArgs, platform),
    })
  }, [])

  const answerPrompt = (yes: boolean) => {
    dispatchHistory({ type: "event", event: { type: "answered" } })
    writeStdin(yes ? "y\r" : "n\r").catch(console.error)
  }

  // Re-read after every job: the user may have just generated or edited it.
  const refreshDefaults = useCallback(() => {
    defaultBitrates().then(setDefaults).catch(console.warn)
    // [default_modes] may have been edited too
    guiSettings()
      .then((settings) => {
        const modes = parseDefaultModes(
          settings.defaultDdpMode,
          settings.defaultDdpAtmosMode,
          settings.defaultAc4ImsMode
        )
        setDefaultModes(modes)
        // a config with ac4_ims = 'music' needs the DRC music_light
        setOptions((o) =>
          o.format === "ac4-ims" ? setAc4ImsMode(o, o.ac4ImsMode, modes) : o
        )
        setDdFilterDefaults(ddFilterDefaultsOf(settings))
        setConfigTemplate(outputTemplateOf(settings))
      })
      .catch(console.warn)
  }, [])

  useEffect(() => {
    refreshDefaults()
  }, [refreshDefaults])

  // Shows an empty prompt unless one is already there.
  const showPrompt = useCallback((term: TerminalHandle) => {
    if (promptShownRef.current) return
    promptShownRef.current = true
    term.write(prompt(versionRef.current))
  }, [])

  // The first prompt, once the version is known (or failed to load).
  useEffect(() => {
    let cancelled = false
    void loadVersion().then(() => {
      // A job attached in the meantime has written its own prompt.
      if (!cancelled && terminal.current && !runningRef.current) {
        showPrompt(terminal.current)
      }
    })
    return () => {
      cancelled = true
    }
  }, [loadVersion, showPrompt])

  // Writes a status line, making sure it starts on a fresh line.
  const writeStatus = useCallback(
    async (term: TerminalHandle, text: string) => {
      await term.flush()
      if (term.cursorX() !== 0) term.write("\r\n")
      term.write(`${text}\r\n`)
    },
    []
  )

  // The status line of a finished job, then a blank line and a fresh prompt.
  const finish = useCallback(
    async (term: TerminalHandle, status: string) => {
      await writeStatus(term, status)
      term.write("\r\n")
      showPrompt(term)
      refreshDefaults()
    },
    [writeStatus, showPrompt, refreshDefaults]
  )

  // The prompt line with the arguments of a job that is starting / running.
  const printStart = useCallback(
    async (term: TerminalHandle, runArgs: string[]) => {
      await loadVersion()
      await term.flush()
      // The args go after the prompt that is already shown.
      showPrompt(term)
      promptShownRef.current = false
      term.write(`${formatCommand(runArgs, platform)}\r\n`)
    },
    [loadVersion, showPrompt]
  )

  // The job ended (any mode): status line, blank line and a fresh prompt.
  // `code` is undefined when the outcome is unknown (connection lost).
  // Web: fires if the server never acknowledges a start (dead socket).
  const startTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined
  )
  const clearStartTimer = useCallback(() => {
    clearTimeout(startTimerRef.current)
    startTimerRef.current = undefined
  }, [])
  useEffect(() => clearStartTimer, [clearStartTimer])

  const endRun = useCallback(
    (term: TerminalHandle, code: number | null | undefined) => {
      // Exit code 130 is deew's answer to Ctrl-C (Stop, or typed into the
      // terminal pane): a stop, not a failure.
      const stopped = stoppedRef.current || code === 130
      const startedAt = startedAtRef.current
      clearStartTimer()
      runningRef.current = false
      awaitingStartRef.current = false
      setRunning(false)
      setStopping(false)
      setOutcome(runOutcome({ stopped, code }))
      setEndCount((n) => n + 1)
      const ms = startedAt === null ? null : performance.now() - startedAt
      const status = stopped
        ? stoppedStatus()
        : code === undefined
          ? exitStatus(null)
          : code !== 0
            ? exitStatus(code)
            : // An attached job's start is unknown, so no duration.
              ms === null
              ? finishedStatus()
              : successStatus(ms)
      dispatchHistory({
        type: "end",
        status: stopped
          ? { kind: "stopped" }
          : code === undefined || code === null
            ? { kind: "exit", code: null }
            : code !== 0
              ? { kind: "exit", code }
              : { kind: "success", ms },
      })
      void finish(term, status)
    },
    [finish, clearStartTimer]
  )

  const failRun = useCallback(
    (term: TerminalHandle, message: string) => {
      clearStartTimer()
      runningRef.current = false
      awaitingStartRef.current = false
      setRunning(false)
      setStopping(false)
      setOutcome(runOutcome({ error: true }))
      setEndCount((n) => n + 1)
      dispatchHistory({ type: "end", status: { kind: "error", message } })
      void finish(term, errorStatus(message))
    },
    [finish, clearStartTimer]
  )

  const start = useCallback(
    async (runArgs: string[]) => {
      const term = terminal.current
      // Not next to -gt / -gc (the backends refuse it as well).
      if (!term || runningRef.current || encodeBlock(utilityRef.current)) return
      runningRef.current = true
      awaitingStartRef.current = inWeb()
      stoppedRef.current = false
      setRunning(true)
      setStopping(false)
      beginProgress(runArgs)
      await printStart(term, runArgs)
      // Focusing opens the on-screen keyboard on phones.
      // A hidden terminal must not take the keystrokes.
      const visible = viewRef.current !== "native" && !utilitiesShownRef.current
      if (visible && !window.matchMedia("(pointer: coarse)").matches)
        term.focus()
      startedAtRef.current = performance.now()
      try {
        const { cols, rows } = term.size()
        // Tauri reports through the handlers, the web through `attachWeb`.
        await runDeew(runArgs, cols, rows, true, {
          onOutput: (bytes) => term.write(bytes),
          onExit: (code) => endRun(term, code),
          onProgress: onProgressLine,
        })
        if (awaitingStartRef.current) {
          clearStartTimer()
          startTimerRef.current = setTimeout(() => {
            startTimerRef.current = undefined
            if (awaitingStartRef.current && runningRef.current) {
              failRun(
                term,
                "no response from deew-web (connection lost?), try again"
              )
              reconnectRunSocket()
            }
          }, 5000)
        }
      } catch (error) {
        failRun(term, errorMessage(error))
      }
    },
    [
      printStart,
      endRun,
      failRun,
      clearStartTimer,
      beginProgress,
      onProgressLine,
    ]
  )

  // Web: a job that this tab did not start (page reload, another tab) is
  // shown like a local one: prompt + args, then its output.
  const attachWeb = useCallback(
    async (
      term: TerminalHandle,
      runArgs: string[],
      startedAt: number | null
    ) => {
      runningRef.current = true
      stoppedRef.current = false
      startedAtRef.current = startedAt
      setRunning(true)
      setStopping(false)
      beginProgress(runArgs)
      await printStart(term, runArgs)
    },
    [printStart, beginProgress]
  )

  // Web: the run socket. The server pushes the state on every (re)connect and
  // broadcasts started / exit / output of any job, whoever started it.
  useEffect(() => {
    if (!inWeb()) return
    const enqueue = (
      handle: (term: TerminalHandle) => void | Promise<void>
    ) => {
      eventChain.current = eventChain.current
        .then(() => {
          const term = terminal.current
          return term ? handle(term) : undefined
        })
        .catch(console.error)
    }
    return connectRunEvents({
      onState: (serverRunning, runArgs) =>
        enqueue(async (term) => {
          const action = stateAction(runningRef.current, serverRunning)
          if (action === "attach") {
            await attachWeb(term, runArgs ?? [], null)
          } else if (action === "reattach") {
            // The whole output is replayed: start over.
            term.reset()
            // Stored progress lines follow the state frame.
            dispatchHistory({ type: "restart" })
            promptShownRef.current = false
            await printStart(term, runArgs ?? [])
          } else if (action === "lost") {
            endRun(term, undefined)
          }
        }),
      onStarted: (runArgs) =>
        enqueue(async (term) => {
          clearStartTimer()
          awaitingStartRef.current = false
          // Started by this tab: already shown.
          if (!runningRef.current) {
            await attachWeb(term, runArgs, performance.now())
          }
        }),
      onOutput: (bytes) => enqueue((term) => term.write(bytes)),
      onProgress: (line) => enqueue(() => onProgressLine(line)),
      onExit: (code) =>
        enqueue((term) => {
          if (runningRef.current) endRun(term, code)
        }),
      onError: (message) =>
        enqueue((term) => {
          if (runningRef.current && awaitingStartRef.current) {
            clearStartTimer()
            failRun(term, message)
          } else {
            console.error("deew-web:", message)
          }
        }),
    })
  }, [attachWeb, printStart, endRun, failRun, clearStartTimer, onProgressLine])

  const showUtilities = (shown: boolean) => {
    utilitiesShownRef.current = shown
    setUtilitiesShown(shown)
  }

  const run = () => {
    if (!canRun) return
    // An encode shows the saved output view (CLI/GUI), not the utilities.
    showUtilities(false)
    void start(args)
  }

  // Not saved in the config: the saved view comes back with the other tabs.
  const changeTab = (tab: OutputTab) => {
    if (tab === "utilities") return showUtilities(true)
    showUtilities(false)
    if (tab !== viewRef.current) changeView(tab)
  }

  // The status line of a finished utility, then the slot is free again.
  const endUtility = useCallback(
    async (
      term: TerminalHandle,
      flag: InfoFlag,
      end: { code: number | null } | { error: string }
    ) => {
      const stopped = utilityStoppedRef.current
      const status =
        "error" in end
          ? errorStatus(end.error)
          : stopped || end.code === 130
            ? stoppedStatus()
            : end.code === 0
              ? ""
              : exitStatus(end.code)
      if (status !== "") await writeStatus(term, status)
      else {
        await term.flush()
        if (term.cursorX() !== 0) term.write("\r\n")
      }
      // An update changes the version of deew: read it again for the prompt.
      if (isUpdate(flag)) {
        versionRef.current = null
        await loadVersion()
      }
      void refreshCli()
      // Like the CLI tab: a blank line and a fresh prompt.
      term.write(`\r\n${prompt(versionRef.current)}`)
      utilityRef.current = null
      setUtility(null)
      // -gc / -gt change the config and the tools.
      if (flag === "-gc" || flag === "-gt" || isUpdate(flag)) refreshDefaults()
    },
    [writeStatus, refreshDefaults, loadVersion, refreshCli]
  )

  const runUtility = useCallback(
    async (flag: InfoFlag, confirmed = false) => {
      const term = utilityTerminal.current
      if (!term || utilityRef.current) return
      // Never run a destructive command unless the user confirmed it.
      if (needsConfirmation(flag) && !confirmed) return
      if (
        utilityBlock(flag, {
          encoding: runningRef.current,
          utilityRunning: false,
          cliMissing: cliMissingRef.current,
        })
      )
        return
      utilityRef.current = flag
      utilityStoppedRef.current = false
      setUtility(flag)
      setUtilityUsed(true)
      showUtilities(true)
      // The previous output goes away.
      term.reset()
      // Let the terminal get its real size before deew prints for it.
      await afterLayout()
      await loadVersion()
      term.write(
        `${prompt(versionRef.current)}${formatCommand(infoArgs(flag), platform)}\r\n`
      )
      const { cols, rows } = term.size()
      await runInfo(flag, cols, rows, {
        onOutput: (bytes) => term.write(bytes),
        onExit: (code) => void endUtility(term, flag, { code }),
        onError: (error) => void endUtility(term, flag, { error }),
      })
    },
    [endUtility, loadVersion]
  )

  // "Generate config" and the updates change files: always ask first.
  const requestUtility = (flag: InfoFlag) => {
    if (isUpdate(flag)) setConfirmUpdate(flag)
    else if (needsConfirmation(flag)) setConfirmConfig(true)
    else void runUtility(flag)
  }

  const onTerminalData = useCallback((data: string) => {
    if (runningRef.current) writeStdin(data).catch(console.error)
  }, [])
  const onUtilityData = useCallback((data: string) => {
    if (utilityRef.current) writeInfo(data).catch(console.error)
  }, [])
  const onTerminalResize = useCallback((cols: number, rows: number) => {
    if (runningRef.current) resizePty(cols, rows).catch(console.error)
  }, [])
  const onUtilityResize = useCallback((cols: number, rows: number) => {
    if (utilityRef.current) resizeInfo(cols, rows).catch(console.error)
  }, [])

  return (
    <TooltipProvider delay={400}>
      <div className="flex h-full flex-col">
        {inTauri() && (
          <TitleBar
            nativeTitleBar={nativeTitleBar}
            encoding={running}
            utilityRunning={utility !== null}
            cliMissing={cliMissing}
            onRunInfo={requestUtility}
            onThemeChange={saveTheme}
          />
        )}
        <div className="min-h-0 flex-1 overflow-y-auto overscroll-y-none">
          <div
            className={`flex flex-col gap-3 p-3 ${
              inTauri() ? "min-h-[max(100%,533px)]" : "min-h-[max(100%,560px)]"
            }`}
          >
            <OptionsForm
              value={options}
              onChange={setOptions}
              defaultOutput={defaultOutput}
              dragging={dragging}
              disabled={running}
              probe={probe}
              bitrateInfo={bitrateInfo}
              defaultBitrates={defaults}
              defaultModes={defaultModes}
              ddFilterDefaults={ddFilterDefaults}
              naming={naming}
              outputNames={outputNames}
              downmixChoices={downmixes}
              toolbar={
                inWeb() && (
                  <WebToolbar
                    encoding={running}
                    utilityRunning={utility !== null}
                    cliMissing={cliMissing}
                    onRunInfo={requestUtility}
                    onLogout={onLogout}
                  />
                )
              }
              onAddFiles={async () => addInputs(await pick(false, true))}
              onAddFolders={async () => addInputs(await pick(true, true))}
              onPickOutput={async () => {
                const [dir] = await pick(true, false)
                if (dir) setOptions((o) => ({ ...o, output: dir }))
              }}
            />

            <CommandBar
              command={command}
              problems={problems}
              running={running}
              stopping={stopping}
              runState={running ? "running" : (outcome ?? "idle")}
              endCount={endCount}
              canRun={canRun}
              blockedReason={blockedReason}
              onRun={run}
              onStop={() => {
                stoppedRef.current = true
                setStopping(true)
                void stopDeew().catch((error) => {
                  console.error(error)
                  stoppedRef.current = false
                  setStopping(false)
                })
              }}
              view={activeTab(view, utilitiesShown)}
              onViewChange={changeTab}
            />
            {translocated && (
              <NoteLine
                tone="warning"
                text="macOS runs deew-gui from a temporary copy. Move deew-gui.app to Applications (or your deew folder) with Finder."
              />
            )}
            {saveError !== null && (
              <NoteLine
                tone="warning"
                text={`Setting not saved: ${saveError}`}
              />
            )}

            <div
              className="flex flex-1 flex-col gap-2"
              style={{ minHeight: outputMinHeight }}
            >
              {!utilitiesShown && !showTerminal && (
                <ProgressPane className="min-h-24 flex-1" runs={history.runs} />
              )}
              {/* Always mounted, only hidden: remounting would lose the xterm. */}
              <div
                className={
                  !utilitiesShown && showTerminal
                    ? "relative min-h-0 flex-1"
                    : "hidden"
                }
              >
                <TerminalPane
                  ref={terminal}
                  className="absolute inset-0 overflow-hidden rounded-lg border border-input p-2"
                  onData={onTerminalData}
                  onResize={onTerminalResize}
                />
              </div>
              <UtilitiesPane
                ref={utilityTerminal}
                onData={onUtilityData}
                className={utilitiesShown ? "min-h-0 flex-1" : "hidden"}
                running={utility !== null}
                empty={!utilityUsed}
                onStop={() => {
                  utilityStoppedRef.current = true
                  void stopInfo().catch(console.error)
                }}
                onResize={onUtilityResize}
              />
            </div>
          </div>
        </div>
      </div>
      <ConfirmConfigDialog
        open={confirmConfig}
        onCancel={() => setConfirmConfig(false)}
        onConfirm={() => {
          setConfirmConfig(false)
          void runUtility("-gc", true)
        }}
      />
      <ConfirmUpdateDialog
        request={
          confirmUpdate === null ? null : { all: confirmUpdate === "-u all" }
        }
        onCancel={() => setConfirmUpdate(null)}
        onConfirm={() => {
          const flag = confirmUpdate
          setConfirmUpdate(null)
          if (flag !== null) void runUtility(flag, true)
        }}
      />
      <FileBrowserDialog request={browseRequest} />
      <PromptDialog
        question={native ? (current?.progress.prompt ?? null) : null}
        onAnswer={answerPrompt}
      />
    </TooltipProvider>
  )
}

export default App

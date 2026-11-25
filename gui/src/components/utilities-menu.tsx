import { Fragment } from "react"
import { Wrench } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { INFO_COMMANDS, type InfoFlag } from "@/lib/deew-args"
import { HELP } from "@/lib/help-text"
import { utilityBlock } from "@/lib/utilities"

/**
 * The trigger of the Utilities tab. `encoding`: an encode runs, so -gt and -gc
 * are greyed out; `utilityRunning`: one utility runs at a time.
 */
export function UtilitiesMenu({
  encoding,
  utilityRunning,
  cliMissing = false,
  onRun,
}: {
  encoding: boolean
  utilityRunning: boolean
  cliMissing?: boolean
  onRun: (flag: InfoFlag) => void
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="xs"
            className="h-5 px-1.5 text-[0.7rem]"
          />
        }
      >
        <Wrench data-icon="inline-start" />
        Utilities
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-52">
        {INFO_COMMANDS.map(({ flag, label, group }, i) => {
          const block = utilityBlock(flag, {
            encoding,
            utilityRunning,
            cliMissing,
          })
          return (
            <Fragment key={flag}>
              {i > 0 && INFO_COMMANDS[i - 1].group !== group && (
                <DropdownMenuSeparator />
              )}
              {/* The tooltip is on the group: a disabled item gets no pointer events. */}
              <DropdownMenuGroup title={block ?? HELP.info[flag]}>
                <DropdownMenuItem
                  disabled={block !== null}
                  onClick={() => onRun(flag)}
                >
                  {label}
                  <code className="ml-auto font-mono text-xs text-muted-foreground">
                    {flag}
                  </code>
                </DropdownMenuItem>
              </DropdownMenuGroup>
            </Fragment>
          )
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

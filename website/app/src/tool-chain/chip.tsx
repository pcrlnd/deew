import type { FlowItem } from "./chains"

export function Chip({ item }: { item: FlowItem }) {
  if (item.type === "arrow") return <span className="arr">{item.text}</span>
  if (item.type === "chip") {
    return (
      <span className={"chip " + item.kind + (item.opt ? " opt" : "")}>
        {item.text}
      </span>
    )
  }
  return (
    <span className="alt">
      {item.rows.map((row, r) => (
        <span className="alt-row" key={r}>
          {row.map((it, i) => (
            <Chip key={i} item={it} />
          ))}
        </span>
      ))}
    </span>
  )
}

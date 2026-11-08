import { describe, expect, it } from "vitest"

import { balancedRowSizes, columnsThatFit } from "./balanced-rows"

describe("balancedRowSizes", () => {
  it("spreads items evenly, first rows take the extra", () => {
    expect(balancedRowSizes(10, 5)).toEqual([5, 5])
    expect(balancedRowSizes(10, 4)).toEqual([4, 3, 3])
    expect(balancedRowSizes(10, 3)).toEqual([3, 3, 2, 2])
    expect(balancedRowSizes(10, 2)).toEqual([2, 2, 2, 2, 2])
    expect(balancedRowSizes(10, 1)).toEqual(Array(10).fill(1))
    expect(balancedRowSizes(13, 5)).toEqual([5, 4, 4])
    expect(balancedRowSizes(13, 4)).toEqual([4, 3, 3, 3])
  })

  it("handles edge cases", () => {
    expect(balancedRowSizes(10, 12)).toEqual([10])
    expect(balancedRowSizes(0, 4)).toEqual([])
    expect(balancedRowSizes(7, 0)).toEqual(Array(7).fill(1))
  })
})

describe("columnsThatFit", () => {
  it("counts columns including gaps", () => {
    expect(columnsThatFit(1228, 236, 12)).toBe(5)
    expect(columnsThatFit(1227, 236, 12)).toBe(4)
  })

  it("falls back to one column", () => {
    expect(columnsThatFit(0, 236, 12)).toBe(1)
    expect(columnsThatFit(100, 236, 12)).toBe(1)
    expect(columnsThatFit(NaN, 236, 12)).toBe(1)
  })
})

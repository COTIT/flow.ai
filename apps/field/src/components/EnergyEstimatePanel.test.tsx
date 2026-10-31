import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'

import { EnergyEstimatePanel } from '@/components/EnergyEstimatePanel'
import '@/i18n'

const mill = { ratedPowerKw: 15, quantity: 1, hoursPerDay: 6, daysPerWeek: 5 }

describe('EnergyEstimatePanel', () => {
  test('shows the inputs it was given', () => {
    render(<EnergyEstimatePanel {...mill} />)
    expect(screen.getByTestId('estimate-rated-power')).toHaveTextContent('15.000 kW')
    expect(screen.getByTestId('estimate-quantity')).toHaveTextContent('1')
    expect(screen.getByTestId('estimate-hours')).toHaveTextContent('6')
    expect(screen.getByTestId('estimate-days')).toHaveTextContent('5')
  })

  test('shows the three outputs at the columns own precision', () => {
    render(<EnergyEstimatePanel {...mill} />)
    expect(screen.getByTestId('estimate-power')).toHaveTextContent('15.000 kW')
    expect(screen.getByTestId('estimate-kwh-day')).toHaveTextContent('90.000 kWh')
    expect(screen.getByTestId('estimate-kwh-week')).toHaveTextContent('450.000 kWh')
  })

  // "Always labelled as an estimate." A model figure must never read as a
  // measurement or a commitment.
  test('is labelled an estimate', () => {
    render(<EnergyEstimatePanel {...mill} />)
    expect(screen.getByTestId('estimate-panel')).toHaveTextContent(/estimate/i)
  })

  test('says it is not a measurement', () => {
    render(<EnergyEstimatePanel {...mill} />)
    expect(screen.getByTestId('estimate-panel')).not.toHaveTextContent(/measured consumption/i)
    expect(screen.getByTestId('estimate-basis')).toBeInTheDocument()
  })

  test('recalculates when the assumptions change', () => {
    const { rerender } = render(<EnergyEstimatePanel {...mill} />)
    expect(screen.getByTestId('estimate-kwh-day')).toHaveTextContent('90.000 kWh')

    rerender(<EnergyEstimatePanel {...mill} hoursPerDay={8} />)
    expect(screen.getByTestId('estimate-kwh-day')).toHaveTextContent('120.000 kWh')
    expect(screen.getByTestId('estimate-kwh-week')).toHaveTextContent('600.000 kWh')
    // Peak power is unchanged by hours.
    expect(screen.getByTestId('estimate-power')).toHaveTextContent('15.000 kW')
  })

  test('a blank assumption shows zero, never NaN', () => {
    render(<EnergyEstimatePanel {...mill} hoursPerDay={Number.NaN} />)
    expect(screen.getByTestId('estimate-kwh-day')).toHaveTextContent('0.000 kWh')
    expect(screen.getByTestId('estimate-kwh-day')).not.toHaveTextContent(/NaN/)
  })

  test('figures are set for tabular alignment, since they stack', () => {
    render(<EnergyEstimatePanel {...mill} />)
    expect(screen.getByTestId('estimate-power').className).toMatch(/tabular/)
  })
})

/**
 * The arithmetic, shown as arithmetic. A two-column list of labels and figures
 * said what the numbers were; it did not say that the second follows from the
 * first. Three equation lines do, and an estimate that shows its working is
 * harder to mistake for a measurement.
 */
describe('EnergyEstimatePanel shows its working', () => {
  test('each line reads as an equation ending in its result', () => {
    render(<EnergyEstimatePanel {...mill} />)
    const panel = screen.getByTestId('estimate-panel')

    const lines = panel.querySelectorAll('[data-equation]')
    expect([...lines].map((l) => l.getAttribute('data-equation'))).toEqual(['peak', 'day', 'week'])

    expect(lines[0]).toHaveTextContent('15.000 kW')
    expect(lines[0]).toHaveTextContent('=')
    expect(lines[1]).toHaveTextContent('90.000 kWh')
    expect(lines[2]).toHaveTextContent('450.000 kWh')
  })

  test('the day line starts from the peak the line above produced', () => {
    render(<EnergyEstimatePanel {...mill} />)
    const day = screen.getByTestId('estimate-panel').querySelector('[data-equation="day"]')
    expect(day).toHaveTextContent('15.000 kW')
    expect(day).toHaveTextContent('6')
  })

  test('each result carries its unit mark', () => {
    render(<EnergyEstimatePanel {...mill} />)
    for (const testId of ['estimate-power', 'estimate-kwh-day', 'estimate-kwh-week']) {
      expect(screen.getByTestId(testId).querySelector('svg'), testId).not.toBeNull()
    }
  })

  // The band is a solid ground so the text on it stays readable; the word
  // "estimate" is what carries the meaning.
  test('the header band is solid, and tagged', () => {
    render(<EnergyEstimatePanel {...mill} />)
    const header = screen.getByTestId('estimate-panel').querySelector('header')

    expect(header?.getAttribute('style') ?? '').not.toMatch(/gradient|hatch/)
    expect(header).toHaveTextContent(/estimate/i)
  })
})

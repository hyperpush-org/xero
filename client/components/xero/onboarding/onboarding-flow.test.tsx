import { describe, expect, it } from 'vitest'
import { computeStepOrder } from '@/components/xero/onboarding/onboarding-flow'

describe('computeStepOrder', () => {
  it('returns the base order', () => {
    const ids = computeStepOrder(false).map((step) => step.id)
    expect(ids).toEqual(['welcome', 'providers', 'project', 'confirm', 'beta'])
  })

  it('inserts environment-access before confirm when permission requests are present', () => {
    const ids = computeStepOrder(true).map((step) => step.id)
    expect(ids).toEqual([
      'welcome',
      'providers',
      'project',
      'environment-access',
      'confirm',
      'beta',
    ])
  })
})

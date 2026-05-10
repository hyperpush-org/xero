import { describe, expect, it } from 'vitest'
import { computeStepOrder } from '@/components/xero/onboarding/onboarding-flow'

describe('computeStepOrder', () => {
  it('returns the base order without local-environment when launchMode is null', () => {
    const ids = computeStepOrder(null, false).map((step) => step.id)
    expect(ids).toEqual(['welcome', 'providers', 'project', 'notifications', 'confirm'])
  })

  it('inserts local-environment after welcome and before providers when launchMode is local-source', () => {
    const ids = computeStepOrder('local-source', false).map((step) => step.id)
    expect(ids).toEqual([
      'welcome',
      'local-environment',
      'providers',
      'project',
      'notifications',
      'confirm',
    ])
  })

  it('does not insert local-environment for unknown launch modes', () => {
    const ids = computeStepOrder('something-else', false).map((step) => step.id)
    expect(ids).not.toContain('local-environment')
  })

  it('inserts environment-access before confirm when permission requests are present', () => {
    const ids = computeStepOrder(null, true).map((step) => step.id)
    expect(ids).toEqual([
      'welcome',
      'providers',
      'project',
      'notifications',
      'environment-access',
      'confirm',
    ])
  })

  it('combines local-environment and environment-access correctly', () => {
    const ids = computeStepOrder('local-source', true).map((step) => step.id)
    expect(ids).toEqual([
      'welcome',
      'local-environment',
      'providers',
      'project',
      'notifications',
      'environment-access',
      'confirm',
    ])
  })

  it('marks the local-environment step with a step indicator', () => {
    const localEnv = computeStepOrder('local-source', false).find(
      (step) => step.id === 'local-environment',
    )
    expect(localEnv?.showIndicator).toBe(true)
  })
})

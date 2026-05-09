import { describe, expect, it } from 'vitest'

import { shouldValidateCommandResponse } from './xero-desktop'

describe('xero desktop command contract validation', () => {
  it('keeps large catalog response validation in dev/test and skips it in production', () => {
    expect(shouldValidateCommandResponse('list_skill_registry', 'test')).toBe(true)
    expect(shouldValidateCommandResponse('get_provider_model_catalog', 'development')).toBe(true)
    expect(shouldValidateCommandResponse('list_skill_registry', 'production')).toBe(false)
    expect(shouldValidateCommandResponse('get_provider_model_catalog', 'production')).toBe(false)
    expect(shouldValidateCommandResponse('get_repository_status', 'production')).toBe(true)
  })
})

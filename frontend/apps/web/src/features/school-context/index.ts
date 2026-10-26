import { useQuery } from '@tanstack/react-query'
import { schoolsApi } from './api'
/** Browser school queries live outside the administration workspace. */
export function useSchools() {
  return useQuery({ queryKey: ['schools'], queryFn: schoolsApi.schools })
}
export function useSchoolDirectory() {
  return useQuery({ queryKey: ['directory'], queryFn: schoolsApi.directory })
}

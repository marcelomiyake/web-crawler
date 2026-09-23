{{- define "web-crawler.name" -}}
web-crawler
{{- end -}}

{{- define "web-crawler.labels" -}}
app.kubernetes.io/name: {{ include "web-crawler.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ .Chart.Name }}-{{ .Chart.Version | replace "+" "_" }}
{{- end -}}

{{- define "web-crawler.selectorLabels" -}}
app.kubernetes.io/name: {{ include "web-crawler.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "web-crawler.waitForPostgres" -}}
- name: wait-for-postgres
  image: {{ .Values.postgres.image | quote }}
  imagePullPolicy: IfNotPresent
  command: ["sh", "-ec", "until pg_isready -t 2; do sleep 2; done"]
  env:
    - { name: PGHOST, value: web-crawler-postgres }
    - { name: PGPORT, value: "5432" }
    - { name: PGUSER, value: {{ .Values.postgres.username | quote }} }
    - { name: PGDATABASE, value: {{ .Values.postgres.database | quote }} }
  securityContext:
    runAsNonRoot: true
    runAsUser: 70
    runAsGroup: 70
    allowPrivilegeEscalation: false
    readOnlyRootFilesystem: true
    capabilities: { drop: ["ALL"] }
    seccompProfile: { type: RuntimeDefault }
  resources:
    requests: { cpu: 10m, memory: 16Mi }
    limits: { cpu: 100m, memory: 64Mi }
{{- end -}}

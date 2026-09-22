#!/usr/bin/env bash
# Deploy polygo-bot to Cloud Run, from nothing to a webhook URL.
#
#   PROJECT=my-project ./bot/deploy-cloud-run.sh path/to/app-private-key.pem
#
# Run it from the repository root. It is idempotent: run it again to deploy a new build,
# and again after changing a secret. Everything it creates is free at this size — Cloud
# Run's always-free tier covers roughly 18,000 pull requests a month.
#
#   PROJECT   the Google Cloud project (required)
#   REGION    default us-central1, which is inside the free tier
#   SERVICE   default polygo-bot
#   APP_ID    the GitHub App's id; asked for if unset
#   SECRET    the webhook secret; a fresh random one is made if unset
set -euo pipefail

key="${1:-}"
[ -n "$key" ] && [ -f "$key" ] || { echo "usage: PROJECT=… $0 <app-private-key.pem>" >&2; exit 1; }
: "${PROJECT:?set PROJECT to your Google Cloud project id}"
REGION="${REGION:-us-central1}"
SERVICE="${SERVICE:-polygo-bot}"
IMAGE="${REGION}-docker.pkg.dev/${PROJECT}/polygo/${SERVICE}:$(date +%Y%m%d-%H%M%S)"
command -v gcloud >/dev/null || { echo "gcloud is not installed: https://cloud.google.com/sdk/docs/install" >&2; exit 1; }
[ -f bot/Dockerfile ] || { echo "run this from the repository root" >&2; exit 1; }

if [ -z "${APP_ID:-}" ]; then
  read -rp "GitHub App id: " APP_ID
fi
SECRET="${SECRET:-$(openssl rand -hex 24)}"

echo "▸ enabling the services this needs (once per project)"
gcloud services enable run.googleapis.com cloudbuild.googleapis.com \
  artifactregistry.googleapis.com secretmanager.googleapis.com --project "$PROJECT" --quiet

echo "▸ a place to keep the image"
gcloud artifacts repositories describe polygo --location "$REGION" --project "$PROJECT" >/dev/null 2>&1 ||
  gcloud artifacts repositories create polygo --repository-format=docker \
    --location "$REGION" --project "$PROJECT" --description "polygo images" --quiet

echo "▸ the App's key and webhook secret, in Secret Manager (never in the image)"
put_secret() { # name, file
  gcloud secrets describe "$1" --project "$PROJECT" >/dev/null 2>&1 ||
    gcloud secrets create "$1" --replication-policy=automatic --project "$PROJECT" --quiet
  gcloud secrets versions add "$1" --data-file="$2" --project "$PROJECT" --quiet >/dev/null
}
put_secret polygo-bot-key "$key"
printf '%s' "$SECRET" | put_secret polygo-bot-webhook-secret /dev/stdin

# Cloud Run's runtime identity has to be allowed to read them.
SA="$(gcloud projects describe "$PROJECT" --format='value(projectNumber)')-compute@developer.gserviceaccount.com"
for s in polygo-bot-key polygo-bot-webhook-secret; do
  gcloud secrets add-iam-policy-binding "$s" --member "serviceAccount:$SA" \
    --role roles/secretmanager.secretAccessor --project "$PROJECT" --quiet >/dev/null
done

echo "▸ building $IMAGE"
gcloud builds submit --config bot/cloudbuild.yaml --substitutions="_IMAGE=${IMAGE}" \
  --project "$PROJECT" --quiet .

echo "▸ deploying"
# --concurrency=1 because a review needs ~150 MB and two at once would not fit in 512;
# Cloud Run runs more instances instead. --max-instances caps what a busy day can cost.
# --timeout is generous: GitHub stops waiting after 10s but the platform lets the handler
# finish, and the review still gets posted.
gcloud run deploy "$SERVICE" \
  --image "$IMAGE" \
  --region "$REGION" \
  --project "$PROJECT" \
  --platform managed \
  --allow-unauthenticated \
  --memory 512Mi \
  --cpu 1 \
  --concurrency 1 \
  --max-instances 3 \
  --timeout 300 \
  --set-env-vars "POLYGO_BOT_APP_ID=${APP_ID},POLYGO_BOT_SYNC=1" \
  --set-secrets "POLYGO_BOT_PRIVATE_KEY=polygo-bot-key:latest,POLYGO_BOT_WEBHOOK_SECRET=polygo-bot-webhook-secret:latest" \
  --quiet

URL=$(gcloud run services describe "$SERVICE" --region "$REGION" --project "$PROJECT" --format='value(status.url)')
echo
echo "deployed: $URL"
curl -fsS "$URL/health" && echo
echo
echo "Now, in the GitHub App's settings:"
echo "  Webhook URL     $URL/webhook"
echo "  Webhook secret  $SECRET"
echo
echo "Logs:  gcloud run services logs tail $SERVICE --region $REGION --project $PROJECT"

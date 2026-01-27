# HEM HTTP API

HTTP API wrapper for the Home Energy Model (HEM) engine.

## Endpoints

### `GET /health`
Health check endpoint for monitoring.

**Response:**
```json
{
  "status": "healthy",
  "version": "0.1.0"
}
```

### `POST /calculate`
Run a HEM calculation.

**Request Body:**
```json
{
  "input": { /* HEM input JSON according to schema */ },
  "region": "london" // Optional: UK region for weather data (not yet implemented)
}
```

**Response (Success):**
```json
{
  "success": true,
  "data": { /* HEM calculation results */ }
}
```

**Response (Error):**
```json
{
  "success": false,
  "errors": [
    {
      "id": "uuid",
      "status": "422",
      "detail": "Error message"
    }
  ]
}
```

## Deployment

### Railway

1. Connect your GitHub repository to Railway
2. Railway will automatically detect the Dockerfile
3. Set `PORT=8080` environment variable (default)
4. Deploy

### Environment Variables

- `PORT` - HTTP server port (default: 8080)
- `RUST_LOG` - Log level (default: `hem_http=info,tower_http=info`)

## Input Schema

The input JSON must conform to the HEM input schema. See the `schemas/` directory for:
- `input_core.schema.json` - Core HEM input schema
- `input_fhs.schema.json` - Future Homes Standard schema
- `input_core_allowing_fhs.schema.json` - Combined schema

## Weather Data

Currently uses a bundled UK weather file (London). Future versions will support region-specific weather files.

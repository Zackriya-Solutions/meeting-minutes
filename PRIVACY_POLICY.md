# Meetily Privacy Policy

*Last updated: [Current Date]*

## Our Privacy-First Commitment

Meetily is built on the principle that your meeting data should remain private and under your control. This privacy policy explains how we handle data in our open-source meeting assistant.

## Data Processing Philosophy

### Local-First Processing
- **Meeting transcription**: Processed entirely on your device using local Whisper models
- **Audio recordings**: Never transmitted to external servers
- **Meeting content**: Remains on your infrastructure
- **AI summaries**: Generated locally or through your chosen LLM provider

### Your Data Ownership
- You own all meeting data, transcripts, and recordings
- Data is stored locally on your device
- No vendor lock-in - export your data anytime
- Complete control over data retention and deletion

## Usage Analytics

### What We Collect
To improve Meetily and ensure optimal performance, we collect minimal, anonymized usage data:

**Application Usage:**
- Feature usage patterns (which tools you use most)
- Session duration and frequency
- Performance metrics (transcription success rates, error frequencies)
- UI interaction patterns (button clicks, navigation flows)

**Technical Metrics:**
- Application version and platform information
- Error logs and crash reports (anonymized)
- Performance benchmarks (processing times, resource usage)

### What We DON'T Collect
We never collect:
- ❌ Meeting content, transcripts, or recordings
- ❌ Personal information or identifiable data
- ❌ File names, meeting titles, or metadata
- ❌ Audio data or voice patterns
- ❌ Participant names or contact information
- ❌ LLM conversations or AI-generated content

### Why We Collect This Data
This analytics collection is necessary for:
- **Product Quality**: Identifying and fixing bugs that impact user experience
- **Performance Optimization**: Understanding resource usage and system bottlenecks
- **Security**: Detecting potential security issues and vulnerabilities
- **Feature Development**: Making data-driven decisions about new features
- **Open Source Sustainability**: Ensuring the project meets user needs effectively

### Analytics Implementation
- **Provider**: PostHog (privacy-focused analytics platform)
- **Anonymization**: All data linked to generated user IDs only - no personal identification
- **Data retention**: 12 months maximum, then automatically deleted
- **Encryption**: All data encrypted in transit using industry-standard protocols
- **Location**: Data processed in accordance with PostHog's privacy policy
- **Access Control**: Strictly limited to core development team members

## Third-Party Services

### LLM Providers (Optional)
If you choose to use external LLM providers:
- **Anthropic Claude**: Subject to Anthropic's privacy policy
- **Groq**: Subject to Groq's privacy policy
- **Local Ollama**: Processed entirely on your device

### Analytics Service (Optional)
- **PostHog**: Used for usage analytics when enabled
- **Data**: Only anonymized usage patterns, no meeting content
- **Control**: Completely optional and user-controlled

## Your Privacy Rights

### Data Control
- **Access**: View all data stored locally on your device
- **Export**: Export your data in standard formats
- **Delete**: Remove all data from your device


### Analytics Transparency
- **Open source**: Full analytics implementation available for review in our source code
- **Questions**: Contact us for any analytics-related concerns

## Data Security

### Local Security
- Data encrypted at rest using your device's security features
- No transmission of sensitive meeting data
- Standard file system permissions protect your data

### Open Source Transparency
- Full source code available for security review
- Community-audited privacy implementations
- No hidden data collection or tracking

## Changes to This Policy

We will notify users of any material changes to this privacy policy through:
- Updates to this document in our GitHub repository
- Release notes for application updates
- In-app notifications for significant privacy changes

## Contact Us

For privacy-related questions or concerns:
- **GitHub Issues**: [Create an issue](https://github.com/Zackriya-Solutions/meeting-minutes/issues)
- **Email**: [Contact form](https://www.zackriya.com/service-interest-form/)
- **Community**: [Discord](https://discord.gg/crRymMQBFH)

## Open Source Commitment

As an open-source project under MIT license, you can:
- Review our complete privacy implementation
- Modify data handling to meet your requirements
- Deploy entirely on your own infrastructure
- Contribute to privacy improvements

---

*This privacy policy applies to Meetily v0.0.5 and later versions. For enterprise deployments, additional privacy controls may be available.*